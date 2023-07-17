use ed25519_dalek::{PublicKey as SignaturePublicKey, SecretKey as SignatureStaticKey};
use regex::Regex;
use serde::{Serialize, Deserialize};
use std::{net::SocketAddr};
use std::sync::Arc;
use tokio::sync::Mutex;
use x25519_dalek::{PublicKey as EncryptionPublicKey, StaticSecret as EncryptionStaticKey};
use crate::db::{ShinkaiMessageDB};
use crate::db::db_errors::{ShinkaiMessageDBError};
use crate::network::identities::IdentityType;

use super::identity_network_manager::external_identity_to_profile_data;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewIdentity {
    pub full_identity_name: String,
    pub addr: Option<SocketAddr>,
    pub node_encryption_public_key: EncryptionPublicKey,
    pub node_signature_public_key: SignaturePublicKey,
    pub subidentity_encryption_public_key: Option<EncryptionPublicKey>,
    pub subidentity_signature_public_key: Option<SignaturePublicKey>,
    pub permission_type: IdentityType,
}

impl NewIdentity {
    pub fn new(
        full_identity_name: String,
        node_encryption_public_key: EncryptionPublicKey,
        node_signature_public_key: SignaturePublicKey,
        subidentity_encryption_public_key: Option<EncryptionPublicKey>,
        subidentity_signature_public_key: Option<SignaturePublicKey>,
        permission_type: IdentityType,
    ) -> Self {
        Self {
            full_identity_name,
            addr: None,
            node_encryption_public_key,
            node_signature_public_key,
            subidentity_encryption_public_key,
            subidentity_signature_public_key,
            permission_type,
        }
    }

    pub fn node_identity_name(&self) -> &str {
        self.full_identity_name
            .split('/')
            .next()
            .unwrap_or(&self.full_identity_name)
    }

    pub fn subidentity_name(&self) -> Option<&str> {
        let parts: Vec<&str> = self.full_identity_name.split('/').collect();
        if parts.len() > 1 {
            Some(parts[1])
        } else {
            None
        }
    }
}

pub struct NewIdentityManager {
    pub local_node_name: String,
    pub identities: Vec<NewIdentity>,
    pub db: Arc<Mutex<ShinkaiMessageDB>>,
}

impl NewIdentityManager {
    pub async fn new(db: Arc<Mutex<ShinkaiMessageDB>>, local_node_name: String) -> Result<Self, Box<dyn std::error::Error>> {
        if local_node_name.clone().is_empty() {
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Local node name cannot be empty")));
        }
        match NewIdentityManager::is_valid_node_identity_name_and_no_subidentities(&local_node_name.clone()) == false {
            true => return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Local node name is not valid"))),
            false => (),
        }

        let identities = {
            let db = db.lock().await;
            db.new_load_all_sub_identities(local_node_name.clone())?
        };

        if identities.is_empty() {
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "No identities found in database")));
        }

        Ok(Self { local_node_name, identities, db })
    }

    pub async fn add_subidentity(&mut self, identity: NewIdentity) -> anyhow::Result<()> {
        let db = self.db.lock().await;
        let normalized_identity = NewIdentity::new(
            NewIdentityManager::extract_subidentity(&identity.full_identity_name.clone()),
            identity.node_encryption_public_key.clone(),
            identity.node_signature_public_key.clone(),
            identity.subidentity_encryption_public_key.clone(),
            identity.subidentity_signature_public_key.clone(),
            identity.permission_type.clone(),
        );
        db.new_insert_sub_identity(normalized_identity.clone())?;
        self.identities.push(normalized_identity.clone());
        Ok(())
    }

    pub fn search_identity(&self, full_identity_name: &str) -> Option<NewIdentity> {
        // Extract node name from the full identity name
        let node_name = full_identity_name.split('/').next().unwrap_or(full_identity_name);
    
        // If the node name matches local node, search in self.identities
        if self.local_node_name == node_name {
            self.identities.iter()
                .find(|&identity| identity.full_identity_name == full_identity_name)
                .cloned()
        } else {
            // If not, query the identity network manager
            match external_identity_to_profile_data(node_name.to_string()) {
                Ok(identity_network_manager) => {
                    Some(NewIdentity::new(
                        node_name.to_string(),
                        identity_network_manager.encryption_public_key,
                        identity_network_manager.signature_public_key,
                        None,
                        None,
                        IdentityType::Global,
                    ))
                }
                Err(_) => None, // return None if the identity is not found in the network manager
            }
        }
    }
}

impl NewIdentityManager {
    pub fn extract_subidentity(s: &str) -> String {
        let re = Regex::new(r"@@[^/]+\.shinkai/(.+)").unwrap();
        re.captures(s).and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .unwrap_or_else(|| s.to_string())
    } 

    pub fn extract_node_name(s: &str) -> String {
        let re = Regex::new(r"(@@[^/]+\.shinkai)(?:/.*)?").unwrap();
        re.captures(s)
            .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .unwrap_or_else(|| s.to_string())
    }
    
    pub fn is_valid_node_identity_name_and_no_subidentities(s: &str) -> bool {
        let re = Regex::new(r"^@@[^/]+\.shinkai$").unwrap();
        re.is_match(s)
    }
}