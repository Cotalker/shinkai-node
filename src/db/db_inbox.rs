use chrono::DateTime;
use chrono::Utc;
use rocksdb::{Error, Options, WriteBatch};
use shinkai_message_primitives::shinkai_message::shinkai_message::ExternalMetadata;
use shinkai_message_primitives::shinkai_message::shinkai_message::NodeApiData;
use shinkai_message_primitives::{
    schemas::{inbox_name::InboxName, shinkai_name::ShinkaiName, shinkai_time::ShinkaiStringTime},
    shinkai_message::{shinkai_message::ShinkaiMessage, shinkai_message_schemas::WSTopic},
    shinkai_utils::shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption},
};

use crate::schemas::{
    identity::{IdentityType, StandardIdentity},
    inbox_permission::InboxPermission,
    smart_inbox::SmartInbox,
};

use super::{db::Topic, db_errors::ShinkaiDBError, ShinkaiDB};

impl ShinkaiDB {
    pub fn create_empty_inbox(&self, inbox_name: String) -> Result<(), Error> {
        shinkai_log(
            ShinkaiLogOption::JobExecution,
            ShinkaiLogLevel::Info,
            &format!("Creating inbox: {}", inbox_name),
        );
        // Use shared CFs
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        // Start a write a batch
        let mut batch = WriteBatch::default();

        // Construct keys with inbox_name as part of the key
        let inbox_key = format!("inbox_{}", inbox_name);
        let inbox_read_list_key = format!("{}_read_list", inbox_name); // ADD: this to job as well
        let inbox_smart_inbox_name_key = format!("{}_smart_inbox_name", inbox_name);

        // Content
        let initial_inbox_name = format!("New Inbox: {}", inbox_name);

        // Put Inbox Data into the DB
        batch.put_cf(cf_inbox, inbox_key.as_bytes(), "".as_bytes());
        batch.put_cf(cf_inbox, inbox_read_list_key.as_bytes(), "".as_bytes());
        batch.put_cf(
            cf_inbox,
            inbox_smart_inbox_name_key.as_bytes(),
            initial_inbox_name.as_bytes(),
        );

        eprintln!(">>> Creating inbox: {}", inbox_name);
        // Commit the write batch
        self.db.write(batch)?;
        eprintln!(">>> Inbox created: {}", inbox_name);

        Ok(())
    }

    // This fn doesn't validate access to the inbox (not really a responsibility of this db fn) so it's unsafe in that regard
    pub async fn unsafe_insert_inbox_message(
        &self,
        message: &ShinkaiMessage,
        maybe_parent_message_key: Option<String>,
    ) -> Result<(), ShinkaiDBError> {
        eprintln!("insert message> Message: {:?}", message);

        let inbox_name_manager = InboxName::from_message(message).map_err(ShinkaiDBError::from)?;

        // If the inbox name is empty, use the get_inbox_name function
        let inbox_name = match &inbox_name_manager {
            InboxName::RegularInbox { value, .. } | InboxName::JobInbox { value, .. } => value.clone(),
        };

        // If the inbox name is empty, use the get_inbox_name function
        if inbox_name.is_empty() {
            return Err(ShinkaiDBError::SomeError("Inbox name is empty".to_string()));
        }

        // Use shared CFs
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        // Construct keys with inbox_name as part of the key
        let inbox_key = format!("inbox_{}", inbox_name);
        let fixed_inbox_key = format!("inbox_{}", inbox_name_manager.hash_value_first_half()); 
        eprintln!("insert message> Inbox key: {}", inbox_key);

        // Check if the inbox exists and if not, create it
        if self.db.get_cf(cf_inbox, inbox_key.as_bytes())?.is_none() {
            eprintln!("insert message> Creating inbox: {}", inbox_name);
            if let Err(e) = self.create_empty_inbox(inbox_name.clone()) {
                eprintln!("Error creating inbox: {}", e);
            } else {
                eprintln!("insert message> Inbox created: {}", inbox_name);
            }
        }

        // println!("Hash key: {}", hash_key);

        // Clone the external_metadata first, then unwrap
        let ext_metadata = message.external_metadata.clone();

        // Get the scheduled time or calculate current time
        let time_key = match ext_metadata.scheduled_time.is_empty() {
            true => ShinkaiStringTime::generate_time_now(),
            false => ext_metadata.scheduled_time.clone(),
        };

        // If this message has a parent, add this message as a child of the parent
        let parent_key = match maybe_parent_message_key {
            Some(key) if !key.is_empty() => Some(key),
            _ => {
                // Fetch the most recent message from the inbox
                let last_messages = self.get_last_messages_from_inbox(inbox_name.clone(), 1, None)?;
                if let Some(first_batch) = last_messages.first() {
                    if let Some(last_message) = first_batch.first() {
                        Some(last_message.calculate_message_hash_for_pagination())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        };
        eprintln!("insert message> Parent key: {:?}", parent_key);

        // Note(Nico): We are not going to let add messages older than its parent if it's a JobInbox
        // If the inbox is of type JobInbox, fetch the parent message and compare its scheduled_time
        if let InboxName::JobInbox { .. } = inbox_name_manager {
            if let Some(parent_key) = &parent_key.clone() {
                let (parent_message, _) = self.fetch_message_and_hash(parent_key)?;
                let parent_time = parent_message.external_metadata.scheduled_time;
                let parsed_time_key: DateTime<Utc> = DateTime::parse_from_rfc3339(&time_key)?.into();
                let parsed_parent_time: DateTime<Utc> = DateTime::parse_from_rfc3339(&parent_time)?.into();
                if parsed_time_key < parsed_parent_time {
                    return Err(ShinkaiDBError::SomeError(
                        "Scheduled time of the message is older than its parent".to_string(),
                    ));
                }
            }
        }

        // Calculate the hash of the message for the key
        let hash_key = message.calculate_message_hash_for_pagination();

        // We update the message with some extra information api_node_data
        let updated_message = {
            let node_api_data = NodeApiData {
                parent_hash: parent_key.clone().unwrap_or_default(),
                node_message_hash: hash_key.clone(), // this is safe because hash_key doesn't use node_api_data
                node_timestamp: time_key.clone(),
            };

            let mut updated_message = message.clone();
            updated_message.external_metadata.node_api_data = Some(node_api_data);
            updated_message.clone()
        };

        // Create the composite key by concatenating the time_key and the hash_key, with a separator
        let composite_key = format!("{}_message_{}:::{}", fixed_inbox_key, time_key, hash_key);
        println!("insert message> Composite key: {}", composite_key);

        let mut batch = rocksdb::WriteBatch::default();

        // Add the message to the shared column family with a key that includes the inbox name
        batch.put_cf(cf_inbox, composite_key.as_bytes(), &hash_key);

        // Insert the message
        let _ = self.insert_message_to_all(&updated_message.clone())?;

        // If this message has a parent, add this message as a child of the parent
        if let Some(parent_key) = parent_key {
            // eprintln!("Adding child: {} to parent: {}", composite_key, parent_key);
            // eprintln!("Inbox name: {}", inbox_name);

            // Construct a key for storing child messages of a parent
            let parent_children_key = format!("{}_children_{}", fixed_inbox_key, parent_key);

            // Fetch existing children for the parent, if any
            let existing_children_bytes = self
                .db
                .get_cf(cf_inbox, parent_children_key.as_bytes())?
                .unwrap_or_default();
            let existing_children = String::from_utf8(existing_children_bytes)
                .unwrap()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect::<Vec<String>>();

            let mut children = vec![hash_key.clone()];
            children.extend_from_slice(&existing_children);

            batch.put_cf(cf_inbox, parent_children_key.as_bytes(), children.join(","));

            let message_parent_key = format!("{}_parent_{}", fixed_inbox_key, hash_key);

            // Add the parent key to the parents column family with the child key
            self.db.put_cf(cf_inbox, message_parent_key.as_bytes(), parent_key)?;
        }

        {
            // Note: this is the code for enabling WS
            if let Some(manager) = &self.ws_manager {
                let m = manager.lock().await;
                let inbox_name_string = inbox_name.to_string();
                if let Ok(msg_string) = message.to_string() {
                    let _ = m.queue_message(WSTopic::Inbox, inbox_name_string, msg_string).await;
                }
            }
        }

        self.db.write(batch)?;
        Ok(())
    }

    pub fn mark_as_read_up_to(
        &mut self,
        inbox_name: String,
        up_to_message_hash_offset: String,
    ) -> Result<(), ShinkaiDBError> {
        // Use the Inbox CF for marking messages as read
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        // Construct the key for the read list within the Inbox CF
        let inbox_read_list_key = format!("{}_read_list", inbox_name);

        // Store the up_to_message_hash_offset as the value for the read list key
        // This represents the last message that has been read up to
        self.db.put_cf(
            cf_inbox,
            inbox_read_list_key.as_bytes(),
            up_to_message_hash_offset.as_bytes(),
        )?;

        Ok(())
    }

    pub fn get_last_read_message_from_inbox(&self, inbox_name: String) -> Result<Option<String>, ShinkaiDBError> {
        // Use the Inbox CF for fetching the last read message
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        // Construct the key for the last read message within the Inbox CF
        let inbox_read_list_key = format!("{}_read_list", inbox_name);

        // Directly fetch the value associated with the last read message key
        match self.db.get_cf(cf_inbox, inbox_read_list_key.as_bytes())? {
            Some(value) => {
                // Convert the value to a String
                let last_read_message = String::from_utf8(value.to_vec())
                    .map_err(|_| ShinkaiDBError::SomeError("UTF-8 conversion error".to_string()))?;
                Ok(Some(last_read_message))
            }
            None => Ok(None), // If there's no value, return None
        }
    }

    pub fn get_last_unread_messages_from_inbox(
        &self,
        inbox_name: String,
        n: usize,
        from_offset_hash_key: Option<String>,
    ) -> Result<Vec<ShinkaiMessage>, ShinkaiDBError> {
        // Fetch the last read message
        let last_read_message = self.get_last_read_message_from_inbox(inbox_name.clone())?;

        // Fetch the last n messages from the inbox
        let messages = self.get_last_messages_from_inbox(inbox_name, n, from_offset_hash_key)?;

        // Flatten the Vec<Vec<ShinkaiMessage>> to Vec<ShinkaiMessage>
        let messages: Vec<ShinkaiMessage> = messages.into_iter().flatten().collect();

        // Iterate over the messages in reverse order until you reach the message with the last_read_message
        let mut unread_messages = Vec::new();
        for message in messages.into_iter().rev() {
            if Some(message.calculate_message_hash_for_pagination()) == last_read_message {
                break;
            }
            unread_messages.push(message);
        }

        unread_messages.reverse();
        Ok(unread_messages)
    }

    pub fn add_permission(
        &mut self,
        inbox_name: &str,
        identity: &StandardIdentity,
        perm: InboxPermission,
    ) -> Result<(), ShinkaiDBError> {
        // Call the new function with the extracted profile name
        let shinkai_profile = identity.full_identity_name.extract_profile()?;
        self.add_permission_with_profile(inbox_name, shinkai_profile, perm)
    }

    pub fn add_permission_with_profile(
        &mut self,
        inbox_name: &str,
        profile: ShinkaiName,
        perm: InboxPermission,
    ) -> Result<(), ShinkaiDBError> {
        let profile_name = profile
            .get_profile_name()
            .clone()
            .ok_or(ShinkaiDBError::InvalidIdentityName(profile.to_string()))?;

        let cf_node = self.get_cf_handle(Topic::NodeAndUsers).unwrap();
        let profile_identity_key = format!("identity_key_of_{}", profile_name.clone().to_string());

        // Check if the identity exists
        if self.db.get_cf(cf_node, profile_identity_key.as_bytes())?.is_none() {
            return Err(ShinkaiDBError::IdentityNotFound(format!(
                "Identity not found for: {}",
                profile_name
            )));
        }

        // Handle the original permission addition
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();
        let perms_key = format!("{}_perms_{}", inbox_name, profile_name);
        let perm_val = perm.to_i32().to_string(); // Convert permission to i32 and then to String
        self.db.put_cf(cf_inbox, perms_key.as_bytes(), perm_val)?;
        Ok(())
    }

    pub fn remove_permission(&mut self, inbox_name: &str, identity: &StandardIdentity) -> Result<(), ShinkaiDBError> {
        let profile_name =
            identity
                .full_identity_name
                .get_profile_name()
                .clone()
                .ok_or(ShinkaiDBError::InvalidIdentityName(
                    identity.full_identity_name.to_string(),
                ))?;

        // Adjusted to use Topic::NodeAndUsers for identity existence check
        let cf_node = self.get_cf_handle(Topic::NodeAndUsers).unwrap();
        let profile_identity_key = format!("identity_key_of_{}", profile_name);

        // Check if the identity exists
        if self.db.get_cf(cf_node, profile_identity_key.as_bytes())?.is_none() {
            return Err(ShinkaiDBError::IdentityNotFound(format!(
                "Identity not found for: {}",
                profile_name
            )));
        }

        // Permission removal logic remains the same
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();
        let perms_key = format!("{}_perms_{}", inbox_name, profile_name);
        self.db.delete_cf(cf_inbox, perms_key)?;
        Ok(())
    }

    pub fn has_permission(
        &self,
        inbox_name: &str,
        identity: &StandardIdentity,
        perm: InboxPermission,
    ) -> Result<bool, ShinkaiDBError> {
        let profile_name =
            identity
                .full_identity_name
                .get_profile_name()
                .clone()
                .ok_or(ShinkaiDBError::InvalidIdentityName(
                    identity.full_identity_name.to_string(),
                ))?;

        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        // Construct the permissions key similar to how it's done in add_permission_with_profile
        // TODO: perm_type not used?
        // TODO(?): if it's admin it should be able to access anything :?
        // Construct the permissions key similar to how it's done in add_permission_with_profile
        let perms_key = format!("{}_perms_{}", inbox_name, profile_name);

        // Attempt to fetch the permission value for the constructed key
        match self.db.get_cf(cf_inbox, perms_key.as_bytes())? {
            Some(val) => {
                // Convert the stored permission value back to an integer, then to an InboxPermission enum
                let val_str = String::from_utf8(val.to_vec())
                    .map_err(|_| ShinkaiDBError::SomeError("UTF-8 conversion error".to_string()))?;
                let stored_perm_val = val_str
                    .parse::<i32>()
                    .map_err(|_| ShinkaiDBError::SomeError("Permission value parse error".to_string()))?;
                let stored_perm = InboxPermission::from_i32(stored_perm_val)?;

                // Check if the stored permission is greater than or equal to the requested permission
                Ok(stored_perm >= perm)
            }
            None => {
                // If no permission is found, the identity does not have the requested permission
                Ok(false)
            }
        }
    }

    pub fn get_inboxes_for_profile(
        &self,
        profile_name_identity: StandardIdentity,
    ) -> Result<Vec<String>, ShinkaiDBError> {
        // Fetch the column family for the 'inbox' topic
        let cf_inbox = match self.db.cf_handle(Topic::Inbox.as_str()) {
            Some(cf) => cf,
            None => {
                return Err(ShinkaiDBError::InboxNotFound(format!(
                    "Inbox not found: {}",
                    profile_name_identity
                )))
            }
        };

        // Create an iterator for the 'inbox' topic
        let iter = self.db.iterator_cf(cf_inbox, rocksdb::IteratorMode::Start);

        let mut inboxes = Vec::new();
        for item in iter {
            // Handle the Result returned by the iterator
            match item {
                Ok((key, _)) => {
                    let key_str = String::from_utf8_lossy(&key);
                    if key_str.contains(&profile_name_identity.full_identity_name.to_string()) {
                        inboxes.push(key_str.to_string());
                    } else {
                        // Check if the identity has read permission for the inbox
                        match self.has_permission(&key_str, &profile_name_identity, InboxPermission::Read) {
                            Ok(has_perm) => {
                                if has_perm {
                                    inboxes.push(key_str.to_string());
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        shinkai_log(
            ShinkaiLogOption::API,
            ShinkaiLogLevel::Info,
            &format!("Inboxes: {}", inboxes.join(", ")),
        );
        Ok(inboxes)
    }

    pub fn get_all_smart_inboxes_for_profile(
        &self,
        profile_name_identity: StandardIdentity,
    ) -> Result<Vec<SmartInbox>, ShinkaiDBError> {
        let inboxes = self.get_inboxes_for_profile(profile_name_identity)?;

        let mut smart_inboxes = Vec::new();

        for inbox_id in inboxes {
            shinkai_log(
                ShinkaiLogOption::API,
                ShinkaiLogLevel::Info,
                &format!("Inbox: {}", inbox_id),
            );

            let last_message = self
                .get_last_messages_from_inbox(inbox_id.clone(), 1, None)?
                .into_iter()
                .next()
                .and_then(|mut v| v.pop());

            let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();
            let inbox_smart_inbox_name_key = format!("{}_smart_inbox_name", &inbox_id);
            let custom_name = match self.db.get_cf(cf_inbox, inbox_smart_inbox_name_key.as_bytes())? {
                Some(val) => String::from_utf8(val.to_vec())
                    .map_err(|_| ShinkaiDBError::SomeError("UTF-8 conversion error".to_string()))?,
                None => inbox_id.clone(), // Use the inbox_id as the default value if the custom name is not found
            };

            // Determine if the inbox is finished
            let is_finished = if inbox_id.starts_with("job_inbox::") {
                match InboxName::new(inbox_id.clone())? {
                    InboxName::JobInbox { unique_id, .. } => {
                        let job = self.get_job(&unique_id)?;
                        job.is_finished
                    }
                    _ => false,
                }
            } else {
                false
            };

            let smart_inbox = SmartInbox {
                inbox_id: inbox_id.clone(),
                custom_name,
                last_message,
                is_finished,
            };

            smart_inboxes.push(smart_inbox);
        }

        // Sort the smart_inboxes by the timestamp of the last message
        smart_inboxes.sort_by(|a, b| match (&a.last_message, &b.last_message) {
            (Some(a_msg), Some(b_msg)) => {
                let a_time = DateTime::parse_from_rfc3339(&a_msg.external_metadata.scheduled_time).unwrap();
                let b_time = DateTime::parse_from_rfc3339(&b_msg.external_metadata.scheduled_time).unwrap();
                b_time.cmp(&a_time)
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        Ok(smart_inboxes)
    }

    pub fn update_smart_inbox_name(&mut self, inbox_id: &str, new_name: &str) -> Result<(), ShinkaiDBError> {
        // Fetch the column family for the smart_inbox_name
        let cf_name_smart_inbox_name = format!("{}_smart_inbox_name", inbox_id);
        let cf_smart_inbox_name = self
            .db
            .cf_handle(&cf_name_smart_inbox_name)
            .ok_or(ShinkaiDBError::InboxNotFound(format!("Inbox not found: {}", inbox_id)))?;

        // Update the name in the column family
        self.db.put_cf(cf_smart_inbox_name, inbox_id, new_name)?;

        Ok(())
    }
}
