use async_recursion::async_recursion;
use shinkai_message_primitives::schemas::{
    inbox_name::InboxName, llm_providers::serialized_llm_provider::SerializedLLMProvider, shinkai_name::ShinkaiName
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use crate::{
    db::ShinkaiDB,
    llm_provider::{
        error::LLMProviderError, execution::prompts::prompts::JobPromptGenerator, job::Job, job_manager::JobManager,
    },
    network::ws_manager::WSUpdateHandler,
};

#[derive(Debug, Clone, Default)]
pub struct CronExecutionChainResponse {
    pub summary: String,
    pub needs_links: bool,
}

#[derive(Debug, Clone)]
pub struct CronExecutionState {
    stage: String,
    summary: Option<String>,
    needs_links: Option<bool>,
}

impl JobManager {
    #[async_recursion]
    pub async fn image_analysis_chain(
        _db: Arc<ShinkaiDB>,
        full_job: Job,
        agent_found: Option<SerializedLLMProvider>,
        _execution_context: HashMap<String, String>,
        _user_profile: Option<ShinkaiName>,
        task: String,
        image: String,
        iteration_count: u64,
        max_iterations: u64,
        ws_manager_trait: Option<Arc<Mutex<dyn WSUpdateHandler + Send>>>,
    ) -> Result<(String, HashMap<String, String>), LLMProviderError> {
        if iteration_count > max_iterations {
            return Err(LLMProviderError::InferenceRecursionLimitReached(
                "Image Analysis".to_string(),
            ));
        }

        let agent = match agent_found {
            Some(agent) => agent,
            None => return Err(LLMProviderError::LLMProviderNotFound),
        };

        let image_prompt = JobPromptGenerator::image_to_text_analysis(task, image);
        let inbox_name: Option<InboxName> = match InboxName::get_job_inbox_name_from_params(full_job.job_id.clone()) {
            Ok(name) => Some(name),
            Err(_) => None,
        };
        let response_json =
            JobManager::inference_with_llm_provider(agent.clone(), image_prompt, inbox_name, ws_manager_trait).await?;
        let mut new_execution_context = HashMap::new();

        new_execution_context.insert(
            "previous_step_response".to_string(),
            response_json.response_string.clone(),
        );
        Ok((response_json.response_string.clone(), new_execution_context))
    }
}
