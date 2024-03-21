use crate::{
    db::ShinkaiDB,
    managers::IdentityManager,
    network::{node_error::NodeError, Node},
};
use ed25519_dalek::{SigningKey, VerifyingKey};
use shinkai_message_primitives::{
    schemas::shinkai_name::ShinkaiName,
    shinkai_message::{
        shinkai_message::{MessageBody, MessageData, ShinkaiMessage},
        shinkai_message_extension::EncryptionStatus,
        shinkai_message_schemas::MessageSchemaType,
    },
    shinkai_utils::{
        encryption::{clone_static_secret_key, encryption_public_key_to_string},
        shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption},
        shinkai_message_builder::{ProfileName, ShinkaiMessageBuilder},
        signatures::{clone_signature_secret_key, signature_public_key_to_string},
    },
};
use std::sync::{Arc, Weak};
use std::{io, net::SocketAddr};
use tokio::sync::Mutex;
use x25519_dalek::{PublicKey as EncryptionPublicKey, StaticSecret as EncryptionStaticKey};

use super::network_job_manager_error::NetworkJobQueueError;

pub enum PingPong {
    Ping,
    Pong,
}

pub async fn handle_based_on_message_content_and_encryption(
    node: Weak<Mutex<Node>>,
    message: ShinkaiMessage,
    sender_encryption_pk: x25519_dalek::PublicKey,
    sender_address: SocketAddr,
    sender_profile_name: String,
    my_encryption_secret_key: &EncryptionStaticKey,
    my_signature_secret_key: &SigningKey,
    my_node_profile_name: &str,
    maybe_db: Arc<Mutex<ShinkaiDB>>,
    maybe_identity_manager: Arc<Mutex<IdentityManager>>,
    receiver_address: SocketAddr,
    unsafe_sender_address: SocketAddr,
) -> Result<(), NetworkJobQueueError> {
    let message_body = message.body.clone();
    let message_content = match &message_body {
        MessageBody::Encrypted(body) => &body.content,
        MessageBody::Unencrypted(body) => match &body.message_data {
            MessageData::Encrypted(data) => &data.content,
            MessageData::Unencrypted(data) => &data.message_raw_content,
        },
    };
    let message_encryption_status = message.clone().get_encryption_status();
    println!(
        "{} > handle_based_on_message_content_and_encryption message: {:?} {:?}",
        receiver_address, message, message_encryption_status
    );

    // TODO: if content body encrypted to the node itself then decrypt it and process it.
    match (message_content.as_str(), message_encryption_status) {
        (_, EncryptionStatus::BodyEncrypted) => {
            handle_default_encryption(
                node.clone(),
                message,
                sender_encryption_pk,
                sender_address,
                sender_profile_name,
                my_encryption_secret_key,
                my_signature_secret_key,
                my_node_profile_name,
                receiver_address,
                unsafe_sender_address,
                maybe_db,
                maybe_identity_manager,
            )
            .await
        }
        (_, EncryptionStatus::ContentEncrypted) => {
            // TODO: save to db to send the profile when connected
            println!("{} > Content encrypted", receiver_address);
            handle_network_message_cases(
                node.clone(),
                message,
                sender_encryption_pk,
                sender_address,
                sender_profile_name,
                my_encryption_secret_key,
                my_signature_secret_key,
                my_node_profile_name,
                receiver_address,
                unsafe_sender_address,
                maybe_db,
                maybe_identity_manager,
            )
            .await
        }
        ("Ping", _) => {
            handle_ping(
                sender_address,
                sender_encryption_pk,
                sender_profile_name,
                my_encryption_secret_key,
                my_signature_secret_key,
                my_node_profile_name,
                receiver_address,
                unsafe_sender_address,
                maybe_db,
                maybe_identity_manager,
            )
            .await
        }
        ("ACK", _) => {
            println!("{} > ACK from {:?}", receiver_address, unsafe_sender_address);
            Ok(())
        }
        (_, EncryptionStatus::NotCurrentlyEncrypted) => {
            handle_network_message_cases(
                node.clone(),
                message,
                sender_encryption_pk,
                sender_address,
                sender_profile_name,
                my_encryption_secret_key,
                my_signature_secret_key,
                my_node_profile_name,
                receiver_address,
                unsafe_sender_address,
                maybe_db,
                maybe_identity_manager,
            )
            .await
        }
    }
}

// All the new helper functions here:
pub fn extract_message(bytes: &[u8], receiver_address: SocketAddr) -> io::Result<ShinkaiMessage> {
    ShinkaiMessage::decode_message_result(bytes.to_vec()).map_err(|_| {
        println!("{} > Failed to decode message.", receiver_address);
        io::Error::new(io::ErrorKind::Other, "Failed to decode message")
    })
}

pub fn verify_message_signature(sender_signature_pk: VerifyingKey, message: &ShinkaiMessage) -> io::Result<()> {
    match message.verify_outer_layer_signature(&sender_signature_pk) {
        Ok(is_valid) if is_valid => Ok(()),
        Ok(_) => {
            shinkai_log(
                ShinkaiLogOption::Network,
                ShinkaiLogLevel::Error,
                "Failed to validate outer message's signature",
            );
            shinkai_log(
                ShinkaiLogOption::Network,
                ShinkaiLogLevel::Error,
                &format!(
                    "Sender signature pk: {:?}",
                    signature_public_key_to_string(sender_signature_pk)
                ),
            );
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to validate outer message's signature",
            ))
        }
        Err(_) => {
            println!("Failed to verify signature. Message: {:?}", message);
            println!(
                "Sender signature pk: {:?}",
                signature_public_key_to_string(sender_signature_pk)
            );
            Err(io::Error::new(io::ErrorKind::Other, "Failed to verify signature"))
        }
    }
}

pub async fn handle_ping(
    sender_address: SocketAddr,
    sender_encryption_pk: x25519_dalek::PublicKey,
    sender_profile_name: String,
    my_encryption_secret_key: &EncryptionStaticKey,
    my_signature_secret_key: &SigningKey,
    my_node_profile_name: &str,
    receiver_address: SocketAddr,
    unsafe_sender_address: SocketAddr,
    maybe_db: Arc<Mutex<ShinkaiDB>>,
    maybe_identity_manager: Arc<Mutex<IdentityManager>>,
) -> Result<(), NetworkJobQueueError> {
    println!("{} > Got ping from {:?}", receiver_address, unsafe_sender_address);
    ping_pong(
        (sender_address, sender_profile_name.clone()),
        PingPong::Pong,
        clone_static_secret_key(my_encryption_secret_key),
        clone_signature_secret_key(my_signature_secret_key),
        sender_encryption_pk,
        my_node_profile_name.to_string(),
        sender_profile_name,
        maybe_db,
        maybe_identity_manager,
    )
    .await
}

pub async fn handle_default_encryption(
    node: Weak<Mutex<Node>>,
    message: ShinkaiMessage,
    sender_encryption_pk: x25519_dalek::PublicKey,
    sender_address: SocketAddr,
    sender_profile_name: String,
    my_encryption_secret_key: &EncryptionStaticKey,
    my_signature_secret_key: &SigningKey,
    my_node_profile_name: &str,
    receiver_address: SocketAddr,
    unsafe_sender_address: SocketAddr,
    maybe_db: Arc<Mutex<ShinkaiDB>>,
    maybe_identity_manager: Arc<Mutex<IdentityManager>>,
) -> Result<(), NetworkJobQueueError> {
    let decrypted_message_result = message.decrypt_outer_layer(&my_encryption_secret_key, &sender_encryption_pk);
    match decrypted_message_result {
        Ok(decrypted_message) => {
            // println!(
            //     "{} > Got message from {:?}. Sending ACK",
            //     receiver_address, unsafe_sender_address
            // );

            // Save to db
            {
                Node::save_to_db(
                    false,
                    &decrypted_message,
                    clone_static_secret_key(&my_encryption_secret_key),
                    maybe_db.clone(),
                    maybe_identity_manager.clone(),
                )
                .await?;
            }

            let message = decrypted_message.get_message_content();
            match message {
                Ok(message_content) => {
                    if message_content != "ACK" {
                        // Call handle_other_cases after decrypting the payload
                        handle_network_message_cases(
                            node,
                            decrypted_message,
                            sender_encryption_pk,
                            sender_address,
                            sender_profile_name,
                            my_encryption_secret_key,
                            my_signature_secret_key,
                            my_node_profile_name,
                            receiver_address,
                            unsafe_sender_address,
                            maybe_db,
                            maybe_identity_manager,
                        )
                        .await?;
                    }
                }
                Err(_) => {
                    // Note(Nico): if we can't decrypt the inner content (it's okay). We still send an ACK
                    let _ = send_ack(
                        (sender_address.clone(), sender_profile_name.clone()),
                        clone_static_secret_key(my_encryption_secret_key),
                        clone_signature_secret_key(my_signature_secret_key),
                        sender_encryption_pk,
                        my_node_profile_name.to_string(),
                        sender_profile_name,
                        maybe_db,
                        maybe_identity_manager,
                    )
                    .await;
                }
            }
            Ok(())
        }
        Err(_) => {
            println!("handle_default_encryption > Failed to decrypt message.");
            // TODO: send error back?
            Ok(())
        }
    }
}

pub async fn handle_network_message_cases(
    node: Weak<Mutex<Node>>,
    message: ShinkaiMessage,
    sender_encryption_pk: x25519_dalek::PublicKey,
    sender_address: SocketAddr,
    sender_profile_name: String,
    my_encryption_secret_key: &EncryptionStaticKey,
    my_signature_secret_key: &SigningKey,
    my_node_profile_name: &str,
    receiver_address: SocketAddr,
    unsafe_sender_address: SocketAddr,
    maybe_db: Arc<Mutex<ShinkaiDB>>,
    maybe_identity_manager: Arc<Mutex<IdentityManager>>,
) -> Result<(), NetworkJobQueueError> {
    println!(
        "{} > Got message from {:?}. Processing and sending ACK",
        receiver_address, unsafe_sender_address
    );
    // Save to db
    // TODO: should this be saved to the networkjobqueue instead?
    {
        Node::save_to_db(
            false,
            &message,
            clone_static_secret_key(&my_encryption_secret_key),
            maybe_db.clone(),
            maybe_identity_manager.clone(),
        )
        .await?;
    }

    // Check the schema of the message and decide what to do
    // TODO: add handler that checks for the Schema and decides what to do with the message
    // TODO: the message may be need to be added to an internal NetworkJobQueue
    // TODO: Create NetworkJobQueue Struct
    match message.get_message_content_schema() {
        Ok(schema) => match schema {
            MessageSchemaType::AvailableSharedItems => {
                // Handle Schema1 specific logic
                println!("Node {}: Handling Schema1 specific logic", my_node_profile_name);

                // 1.- Get subscription results
                // type: Vec<(String, String, Option<ShinkaiFolderSubscription>) -> String

                // Upgrade the Weak reference to a strong reference
                let mut response = "".to_string();
                if let Some(node_lock) = node.upgrade() {
                    // Lock the node to access its fields
                    let node = node_lock.lock().await;

                    // Access the subscription_manager, which is of type Arc<Mutex<Option<SubscriberManager>>>
                    let subscription_manager_lock = node.subscription_manager.lock().await;

                    // Now, the lock is released, and we can proceed without holding onto the `MutexGuard`
                    if let Some(subscription_manager) = &*subscription_manager_lock {
                        let path = "/"; // Define the path you want to query
                        match subscription_manager.get_cached_shared_folder_tree(path).await {
                            Some(tree) => {
                                // Attempt to serialize FSItemTree to a JSON string
                                let unref_tree = &*tree;
                                match serde_json::to_string(unref_tree) {
                                    Ok(tree_str) => {
                                        println!("Successfully retrieved cached shared folder tree for path: {} with tree: {}", path, tree_str);
                                        response = tree_str;
                                    }
                                    Err(e) => println!("Failed to serialize FSItemTree: {}", e),
                                }
                            }
                            None => {
                                // The requested path is not cached
                                println!("No cached shared folder tree found for path: {}", path);
                            }
                        }
                    } else {
                        println!("Subscription manager is not initialized.");
                    }
                } else {
                    println!("Failed to upgrade node reference.");
                }

                // 1.5- extract info from the original message
                let requester = ShinkaiName::from_shinkai_message_using_sender_subidentity(&message)?;

                let request_node_name = requester.get_node_name();
                let request_profile_name = requester.get_profile_name().unwrap_or("".to_string());

                // 2.- Create message using vecfs_available_shared_items_response
                // Send message back with response
                let msg = ShinkaiMessageBuilder::vecfs_available_shared_items_response(
                    response,
                    clone_static_secret_key(&my_encryption_secret_key),
                    clone_signature_secret_key(&my_signature_secret_key),
                    sender_encryption_pk,
                    my_node_profile_name.to_string(),
                    "".to_string(),       // maybe read from the request message
                    request_node_name,    // read from request message
                    request_profile_name, // read from request message
                )
                .unwrap();

                eprintln!("Sending message back with response {:?}", msg);

                // // 3.- Send message back with response

                // Node::send(
                //     msg,
                //     Arc::new(clone_static_secret_key(&encryption_secret_key)),
                //     peer,
                //     maybe_db,
                //     maybe_identity_manager,
                //     false,
                //     None,
                // );
            }
            _ => {
                // Ignore other schemas
                println!("Ignoring other schemas");
            }
        },
        Err(e) => {
            // Handle error case
            println!("Error getting message schema: {:?}", e);
        }
    }

    send_ack(
        (sender_address.clone(), sender_profile_name.clone()),
        clone_static_secret_key(my_encryption_secret_key),
        clone_signature_secret_key(my_signature_secret_key),
        sender_encryption_pk,
        my_node_profile_name.to_string(),
        sender_profile_name,
        maybe_db,
        maybe_identity_manager,
    )
    .await
}

pub async fn send_ack(
    peer: (SocketAddr, ProfileName),
    encryption_secret_key: EncryptionStaticKey, // not important for ping pong
    signature_secret_key: SigningKey,
    receiver_public_key: EncryptionPublicKey, // not important for ping pong
    sender: ProfileName,
    receiver: ProfileName,
    maybe_db: Arc<Mutex<ShinkaiDB>>,
    maybe_identity_manager: Arc<Mutex<IdentityManager>>,
) -> Result<(), NetworkJobQueueError> {
    let msg = ShinkaiMessageBuilder::ack_message(
        clone_static_secret_key(&encryption_secret_key),
        signature_secret_key,
        receiver_public_key,
        sender,
        receiver,
    )
    .unwrap();

    Node::send(
        msg,
        Arc::new(clone_static_secret_key(&encryption_secret_key)),
        peer,
        maybe_db,
        maybe_identity_manager,
        false,
        None,
    );
    Ok(())
}

// Helper struct to encapsulate sender keys
#[derive(Debug)]
pub struct PublicKeyInfo {
    pub address: SocketAddr,
    pub signature_public_key: VerifyingKey,
    pub encryption_public_key: x25519_dalek::PublicKey,
}

pub async fn ping_pong(
    peer: (SocketAddr, ProfileName),
    ping_or_pong: PingPong,
    encryption_secret_key: EncryptionStaticKey, // not important for ping pong
    signature_secret_key: SigningKey,
    receiver_public_key: EncryptionPublicKey, // not important for ping pong
    sender: ProfileName,
    receiver: ProfileName,
    maybe_db: Arc<Mutex<ShinkaiDB>>,
    maybe_identity_manager: Arc<Mutex<IdentityManager>>,
) -> Result<(), NetworkJobQueueError> {
    let message = match ping_or_pong {
        PingPong::Ping => "Ping",
        PingPong::Pong => "Pong",
    };

    let msg = ShinkaiMessageBuilder::ping_pong_message(
        message.to_owned(),
        clone_static_secret_key(&encryption_secret_key),
        signature_secret_key,
        receiver_public_key,
        sender,
        receiver,
    )
    .unwrap();
    Node::send(
        msg,
        Arc::new(clone_static_secret_key(&encryption_secret_key)),
        peer,
        maybe_db,
        maybe_identity_manager,
        false,
        None,
    );
    Ok(())
}
