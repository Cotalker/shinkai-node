use serde::{Deserialize, Serialize};
use shinkai_message_primitives::{shinkai_message::shinkai_message::ShinkaiMessage, shinkai_utils::job_scope::JobScope};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SmartInbox {
    pub inbox_id: String,
    pub custom_name: String,
    pub last_message: Option<ShinkaiMessage>,
    pub is_finished: bool,
    pub job_scope: Option<JobScope>,
}
