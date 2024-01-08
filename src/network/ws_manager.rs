use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::aead::Aead;
use aes_gcm::Aes256Gcm;
use aes_gcm::KeyInit;
use async_trait::async_trait;
use futures::stream::SplitSink;
use futures::SinkExt;
use shinkai_message_primitives::schemas::inbox_name::InboxName;
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_message_primitives::shinkai_message::shinkai_message::ShinkaiMessage;
use shinkai_message_primitives::shinkai_message::shinkai_message_schemas::WSMessage;
use shinkai_message_primitives::shinkai_message::shinkai_message_schemas::WSTopic;
use shinkai_message_primitives::shinkai_utils::encryption::unsafe_deterministic_encryption_keypair;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption};
use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use tokio::time::sleep;
use warp::ws::Message;
use warp::ws::WebSocket;

use crate::db::ShinkaiDB;

use super::node_shareable_logic::validate_message_main_logic;
use super::Node;
use crate::managers::identity_manager::IdentityManagerTrait;

#[derive(Debug)]
pub enum WebSocketManagerError {
    UserValidationFailed(String),
    AccessDenied(String),
    MissingSharedKey(String)
}

impl fmt::Display for WebSocketManagerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WebSocketManagerError::UserValidationFailed(msg) => write!(f, "User validation failed: {}", msg),
            WebSocketManagerError::AccessDenied(msg) => write!(f, "Access denied: {}", msg),
            WebSocketManagerError::MissingSharedKey(msg) => write!(f, "Missing shared key: {}", msg),
        }
    }
}

impl fmt::Debug for WebSocketManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocketManager")
            .field("connections", &self.connections.keys()) // Only print the keys
            .field("subscriptions", &self.subscriptions)
            .field("shinkai_db", &self.shinkai_db)
            .field("node_name", &self.node_name)
            .field("identity_manager_trait", &"Box<dyn IdentityManagerTrait + Send>") // Print a placeholder
            .finish()
    }
}

#[async_trait]
pub trait WSUpdateHandler {
    async fn queue_message(&self, topic: WSTopic, subtopic: String, update: String);
}

pub struct WebSocketManager {
    connections: HashMap<String, Arc<Mutex<SplitSink<WebSocket, Message>>>>,
    // TODO: maybe the first string should be a ShinkaiName? or at least a shinkai name string
    subscriptions: HashMap<String, HashMap<String, bool>>,
    shared_keys: HashMap<String, String>,
    shinkai_db: Arc<Mutex<ShinkaiDB>>,
    node_name: ShinkaiName,
    identity_manager_trait: Arc<Mutex<Box<dyn IdentityManagerTrait + Send>>>,
    message_queue: Arc<Mutex<VecDeque<(WSTopic, String, String)>>>,
}

impl Clone for WebSocketManager {
    fn clone(&self) -> Self {
        Self {
            connections: self.connections.clone(),
            subscriptions: self.subscriptions.clone(),
            shared_keys: self.shared_keys.clone(),
            shinkai_db: Arc::clone(&self.shinkai_db),
            node_name: self.node_name.clone(),
            identity_manager_trait: Arc::clone(&self.identity_manager_trait),
            message_queue: Arc::clone(&self.message_queue),
        }
    }
}

// TODO: maybe this should run on its own thread
impl WebSocketManager {
    pub async fn new(
        shinkai_db: Arc<Mutex<ShinkaiDB>>,
        node_name: ShinkaiName,
        identity_manager_trait: Arc<Mutex<Box<dyn IdentityManagerTrait + Send>>>,
    ) -> Arc<Mutex<Self>> {
        let manager = Arc::new(Mutex::new(Self {
            connections: HashMap::new(),
            subscriptions: HashMap::new(),
            shared_keys: HashMap::new(),
            shinkai_db,
            node_name,
            identity_manager_trait,
            message_queue: Arc::new(Mutex::new(VecDeque::new())),
        }));

        let manager_clone = Arc::clone(&manager);

        // Spawn the message sender task
        let message_queue_clone = Arc::clone(&manager.lock().await.message_queue);
        tokio::spawn(Self::start_message_sender(manager_clone, message_queue_clone));

        manager
    }

    pub async fn start_message_sender(
        manager: Arc<Mutex<Self>>,
        message_queue: Arc<Mutex<VecDeque<(WSTopic, String, String)>>>,
    ) {
        loop {
            eprintln!("Checking for messages in the queue...");
            // Sleep for a while
            sleep(Duration::from_millis(500)).await;

            // Check if there are any messages in the queue
            let message = {
                let mut queue = message_queue.lock().await;
                queue.pop_front()
            };

            if let Some((topic, subtopic, update)) = message {
                eprintln!("Sending update to topic: {}", topic);
                manager.lock().await.handle_update(topic, subtopic, update).await;
            }
        }
    }

    pub async fn user_validation(&self, shinkai_name: ShinkaiName, message: &ShinkaiMessage) -> bool {
        // Message can't be encrypted at this point
        let is_body_encrypted = message.clone().is_body_currently_encrypted();
        if is_body_encrypted {
            eprintln!("Message body is encrypted, can't validate user: {}", shinkai_name);
            shinkai_log(
                ShinkaiLogOption::DetailedAPI,
                ShinkaiLogLevel::Debug,
                format!("Message body is encrypted, can't validate user: {}", shinkai_name).as_str(),
            );
            return false;
        }

        // Note: we generate a dummy encryption key because the message is not encrypted so we don't need the real key.
        let (dummy_encryption_sk, _) = unsafe_deterministic_encryption_keypair(0);

        let identity_manager_clone = self.identity_manager_trait.clone();
        let result = validate_message_main_logic(
            &dummy_encryption_sk,
            identity_manager_clone,
            &shinkai_name.clone(),
            message.clone(),
            None,
        )
        .await;

        eprintln!("user_validation result: {:?}", result);
        match result {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub async fn has_access(&self, shinkai_name: ShinkaiName, topic: WSTopic, subtopic: Option<String>) -> bool {
        // TODO: create enum with all the different topic and subtopics
        // Check if the user has access to the topic and subtopic here...
        eprintln!("has_access> shinkai_name: {}", shinkai_name);
        eprintln!("has_access> topic: {}", topic);
        match topic {
            WSTopic::Inbox => {
                let subtopic = subtopic.unwrap_or_default();
                eprintln!("subtopic: {}", subtopic);
                let inbox_name = InboxName::new(subtopic.clone()).unwrap(); // TODO: handle error
                let sender_subidentity = {
                    let identity_manager_lock = self.identity_manager_trait.lock().await;
                    identity_manager_lock
                        .find_by_identity_name(shinkai_name.clone())
                        .unwrap()
                        .clone() // TODO: handle error
                };
                eprintln!("sender_subidentity: {:?}", sender_subidentity);

                match Node::has_inbox_access(self.shinkai_db.clone(), &inbox_name, &sender_subidentity).await {
                    Ok(_) => {
                        eprintln!(
                            "Access granted for inbox: {} and sender_subidentity: {}",
                            inbox_name, shinkai_name.full_name
                        );
                        return true;
                    }
                    Err(_) => {
                        eprintln!(
                            "Access denied for inbox: {} and sender_subidentity: {}",
                            inbox_name, shinkai_name.full_name
                        );
                        return false;
                    }
                }
            }
            WSTopic::SmartInboxes => {
                eprintln!("smart_inboxes");
                return true;
            }
        }
    }

    pub async fn manage_connections(
        &mut self,
        shinkai_name: ShinkaiName,
        message: ShinkaiMessage,
        connection: Arc<Mutex<SplitSink<WebSocket, Message>>>,
        ws_message: WSMessage,
    ) -> Result<(), WebSocketManagerError> {
        eprintln!("Adding connection for shinkai_name: {}", shinkai_name);
        eprintln!("add_connection> Message: {:?}", message);

        if !self.user_validation(shinkai_name.clone(), &message).await {
            eprintln!("User validation failed for shinkai_name: {}", shinkai_name);
            return Err(WebSocketManagerError::UserValidationFailed(format!(
                "User validation failed for shinkai_name: {}",
                shinkai_name
            )));
        }

        let shinkai_profile_name = shinkai_name.to_string();
        let shared_key = ws_message.shared_key.clone();

        // Initialize the topic map for the new connection
        let mut topic_map = HashMap::new();

        // Iterate over the subscriptions to check access and add them to the topic map
        for subscription in ws_message.subscriptions.iter() {
            if !self
                .has_access(
                    shinkai_name.clone(),
                    subscription.topic.clone(),
                    subscription.subtopic.clone(),
                )
                .await
            {
                eprintln!(
                    "Access denied for shinkai_name: {} on topic: {:?} and subtopic: {:?}",
                    shinkai_name, subscription.topic, subscription.subtopic
                );
                // TODO: should we send a ShinkaiMessage with an error inside back?
                return Err(WebSocketManagerError::AccessDenied(format!(
                    "Access denied for shinkai_name: {} on topic: {:?} and subtopic: {:?}",
                    shinkai_name, subscription.topic, subscription.subtopic
                )));
            }

            let topic_subtopic = format!(
                "{}:::{}",
                subscription.topic,
                subscription.subtopic.clone().unwrap_or_default()
            );
            eprintln!("Subscribing to topic_subtopic: {:?}", topic_subtopic);
            topic_map.insert(topic_subtopic, true);
        }

        // Add the connection and shared key to the manager
        self.connections.insert(shinkai_profile_name.clone(), connection);

        if let Some(key) = shared_key {
            self.shared_keys.insert(shinkai_profile_name.clone(), key);
        } else if !self.shared_keys.contains_key(&shinkai_profile_name) {
            return Err(WebSocketManagerError::MissingSharedKey(format!(
                "Missing shared key for shinkai_name: {}",
                shinkai_profile_name
            )));
        }

        // Handle adding and removing subscriptions
        let subscriptions_to_add: Vec<(WSTopic, Option<String>)> = ws_message
            .subscriptions
            .iter()
            .map(|s| (s.topic.clone(), s.subtopic.clone()))
            .collect();
        let subscriptions_to_remove: Vec<(WSTopic, Option<String>)> = ws_message
            .unsubscriptions
            .iter()
            .map(|s| (s.topic.clone(), s.subtopic.clone()))
            .collect();
        self.update_subscriptions(&shinkai_profile_name, subscriptions_to_add, subscriptions_to_remove)
            .await;

        Ok(())
    }

    // Method to update subscriptions
    pub async fn update_subscriptions(
        &mut self,
        shinkai_name: &str,
        subscriptions_to_add: Vec<(WSTopic, Option<String>)>,
        subscriptions_to_remove: Vec<(WSTopic, Option<String>)>,
    ) {
        // We already checked that the user is allowed to have those subscriptions
        let profile_subscriptions = self.subscriptions.entry(shinkai_name.to_string()).or_default();

        // Add new subscriptions
        for (topic, subtopic) in subscriptions_to_add {
            let key = format!("{}:::{}", topic, subtopic.unwrap_or_default());
            profile_subscriptions.insert(key, true);
        }

        // Remove specified subscriptions
        for (topic, subtopic) in subscriptions_to_remove {
            let key = format!("{}:::{}", topic, subtopic.unwrap_or_default());
            profile_subscriptions.remove(&key);
        }

        // current subscriptions
        let current_subscriptions: Vec<String> = profile_subscriptions.keys().cloned().collect();
        eprintln!("current_subscriptions: {:?}", current_subscriptions);
    }

    pub fn get_all_connections(&self) -> Vec<Arc<Mutex<SplitSink<WebSocket, Message>>>> {
        self.connections.values().cloned().collect()
    }

    pub async fn handle_update(&self, topic: WSTopic, subtopic: String, update: String) {
        let topic_subtopic = format!("{}:::{}", topic, subtopic);
        eprintln!("\n\nSending update to topic: {}", topic_subtopic);
        // Check if the update needs to be sent
        // This is just a placeholder, replace with your actual check
        let needs_to_be_sent = true;

        if needs_to_be_sent {
            // Send the update to all active connections that are subscribed to the topic
            for (id, connection) in self.connections.iter() {
                eprintln!("Checking connection: {}", id);
                if self.subscriptions.get(id).unwrap().get(&topic_subtopic).is_some() {
                    eprintln!("Connection {} is subscribed to the topic", id);
                    let mut connection = connection.lock().await;

                    // Encrypt the update using the shared key
                    let shared_key = self.shared_keys.get(id).unwrap();
                    let shared_key_bytes = hex::decode(shared_key).expect("Failed to decode shared key");
                    let cipher = Aes256Gcm::new(GenericArray::from_slice(&shared_key_bytes));
                    let nonce = GenericArray::from_slice(&[0u8; 12]);
                    let encrypted_update = cipher.encrypt(nonce, update.as_ref()).expect("encryption failure!");
                    let encrypted_update_hex = hex::encode(&encrypted_update);

                    match connection.send(Message::text(encrypted_update_hex.clone())).await {
                        Ok(_) => eprintln!("Successfully sent update to connection {}", id),
                        Err(e) => eprintln!("Failed to send update to connection {}: {}", id, e),
                    }
                } else {
                    eprintln!("Connection {} is not subscribed to the topic {:?}", id, topic_subtopic);
                }
            }
        }
    }
}

#[async_trait]
impl WSUpdateHandler for WebSocketManager {
    async fn queue_message(&self, topic: WSTopic, subtopic: String, update: String) {
        eprintln!("queue_message> topic: {:?}", topic);
        let mut queue = self.message_queue.lock().await;
        queue.push_back((topic, subtopic, update));
    }
}

// Shared reference to WebSocketManager
pub type SharedWebSocketManager = Arc<Mutex<WebSocketManager>>;
