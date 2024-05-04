use serde::{Deserialize, Serialize};
use shinkai_message_primitives::{schemas::{agents::serialized_agent::{AgentLLMInterface, SerializedAgent}, shinkai_name::ShinkaiName}, shinkai_message::shinkai_message::ShinkaiMessage, shinkai_utils::job_scope::JobScope};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AgentSubset {
    pub id: String,
    pub full_identity_name: ShinkaiName,
    pub model: AgentLLMInterface,
}

impl AgentSubset {
    pub fn from_serialized_agent(serialized_agent: SerializedAgent) -> Self {
        Self {
            id: serialized_agent.id,
            full_identity_name: serialized_agent.full_identity_name,
            model: serialized_agent.model,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SmartInbox {
    pub inbox_id: String,
    pub custom_name: String,
    pub last_message: Option<ShinkaiMessage>,
    pub is_finished: bool,
    pub job_scope: Option<JobScope>,
    pub agent: Option<AgentSubset>,
}
