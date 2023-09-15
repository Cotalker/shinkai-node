use super::error::JobManagerError;
use super::IdentityManager;
use crate::agent::agent::Agent;
use crate::agent::job::{Job, JobId, JobLike};
use crate::db::{db_errors::ShinkaiDBError, ShinkaiDB};
use chrono::Utc;
use ed25519_dalek::SecretKey as SignatureStaticKey;
use shinkai_message_primitives::{
    schemas::shinkai_name::{ShinkaiName, ShinkaiNameError},
    shinkai_message::{
        shinkai_message::{MessageBody, MessageData, ShinkaiMessage},
        shinkai_message_schemas::{JobCreationInfo, JobMessage, JobPreMessage, MessageSchemaType},
    },
    shinkai_utils::{shinkai_message_builder::ShinkaiMessageBuilder, signatures::clone_signature_secret_key},
};
use std::fmt;
use std::result::Result::Ok;
use std::{collections::HashMap, error::Error, sync::Arc};
use tokio::sync::{mpsc, Mutex};

pub struct JobManager {
    pub agent_manager: Arc<Mutex<AgentManager>>,
    pub job_manager_receiver: Arc<Mutex<mpsc::Receiver<(Vec<JobPreMessage>, JobId)>>>,
    pub job_manager_sender: mpsc::Sender<(Vec<JobPreMessage>, JobId)>,
    pub identity_secret_key: SignatureStaticKey,
    pub node_profile_name: ShinkaiName,
}

impl JobManager {
    pub async fn new(
        db: Arc<Mutex<ShinkaiDB>>,
        identity_manager: Arc<Mutex<IdentityManager>>,
        identity_secret_key: SignatureStaticKey,
        node_profile_name: ShinkaiName,
    ) -> Self {
        let (job_manager_sender, job_manager_receiver) = tokio::sync::mpsc::channel(100);
        let agent_manager = AgentManager::new(db, identity_manager, job_manager_sender.clone()).await;

        let mut job_manager = Self {
            agent_manager: Arc::new(Mutex::new(agent_manager)),
            job_manager_receiver: Arc::new(Mutex::new(job_manager_receiver)),
            job_manager_sender: job_manager_sender.clone(),
            identity_secret_key,
            node_profile_name,
        };
        job_manager.process_received_messages().await;
        job_manager
    }

    pub async fn process_job_message(&mut self, shinkai_message: ShinkaiMessage) -> Result<String, JobManagerError> {
        let mut agent_manager = self.agent_manager.lock().await;
        if agent_manager.is_job_message(shinkai_message.clone()) {
            agent_manager.process_job_message(shinkai_message).await
        } else {
            Err(JobManagerError::NotAJobMessage)
        }
    }

    pub async fn process_received_messages(&mut self) {
        let agent_manager = Arc::clone(&self.agent_manager);
        let receiver = Arc::clone(&self.job_manager_receiver);
        let node_profile_name_clone = self.node_profile_name.clone();
        let identity_secret_key_clone = clone_signature_secret_key(&self.identity_secret_key);
        tokio::spawn(async move {
            while let Some((messages, job_id)) = receiver.lock().await.recv().await {
                for message in messages {
                    let mut agent_manager = agent_manager.lock().await;

                    let shinkai_message_result = ShinkaiMessageBuilder::job_message_from_agent(
                        job_id.clone(),
                        message.content.clone(),
                        clone_signature_secret_key(&identity_secret_key_clone),
                        node_profile_name_clone.to_string(),
                        node_profile_name_clone.to_string(),
                    );

                    if let Ok(shinkai_message) = shinkai_message_result {
                        if let Err(err) = agent_manager
                            .handle_pre_message_schema(message, job_id.clone(), shinkai_message)
                            .await
                        {
                            eprintln!("Error while handling pre message schema: {:?}", err);
                        }
                    } else if let Err(err) = shinkai_message_result {
                        eprintln!("Error while building ShinkaiMessage: {:?}", err);
                    }
                }
            }
        });
    }

    pub async fn decision_phase(&self, job: &dyn JobLike) -> Result<(), Box<dyn Error>> {
        self.agent_manager.lock().await.decision_phase(job).await
    }

    pub async fn execution_phase(
        &self,
        pre_messages: Vec<JobPreMessage>,
    ) -> Result<Vec<ShinkaiMessage>, Box<dyn Error>> {
        self.agent_manager.lock().await.execution_phase(pre_messages).await
    }
}

pub struct AgentManager {
    jobs: Arc<Mutex<HashMap<String, Box<dyn JobLike>>>>,
    db: Arc<Mutex<ShinkaiDB>>,
    identity_manager: Arc<Mutex<IdentityManager>>,
    job_manager_sender: mpsc::Sender<(Vec<JobPreMessage>, JobId)>,
    agents: Vec<Arc<Mutex<Agent>>>,
}

impl AgentManager {
    pub async fn new(
        db: Arc<Mutex<ShinkaiDB>>,
        identity_manager: Arc<Mutex<IdentityManager>>,
        job_manager_sender: mpsc::Sender<(Vec<JobPreMessage>, JobId)>,
    ) -> Self {
        let jobs_map = Arc::new(Mutex::new(HashMap::new()));
        {
            let shinkai_db = db.lock().await;
            let all_jobs = shinkai_db.get_all_jobs().unwrap();
            let mut jobs = jobs_map.lock().await;
            for job in all_jobs {
                jobs.insert(job.job_id().to_string(), job);
            }
        }

        // Get all serialized_agents and convert them to Agents
        let mut agents: Vec<Arc<Mutex<Agent>>> = Vec::new();
        {
            let identity_manager = identity_manager.lock().await;
            let serialized_agents = identity_manager.get_all_agents().await.unwrap();
            for serialized_agent in serialized_agents {
                let agent = Agent::from_serialized_agent(serialized_agent, job_manager_sender.clone());
                agents.push(Arc::new(Mutex::new(agent)));
            }
        }

        let mut job_manager = Self {
            jobs: jobs_map,
            db,
            job_manager_sender: job_manager_sender.clone(),
            identity_manager,
            agents,
        };

        job_manager
    }

    /// Checks that the provided ShinkaiMessage is an unencrypted job message
    pub fn is_job_message(&mut self, message: ShinkaiMessage) -> bool {
        match &message.body {
            MessageBody::Unencrypted(body) => match &body.message_data {
                MessageData::Unencrypted(data) => match data.message_content_schema {
                    MessageSchemaType::JobCreationSchema | MessageSchemaType::JobMessageSchema => true,
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        }
    }

    /// Processes a job creation message
    pub async fn handle_job_creation_schema(
        &mut self,
        job_creation: JobCreationInfo,
        agent_id: &String,
    ) -> Result<String, JobManagerError> {
        let job_id = format!("jobid_{}", uuid::Uuid::new_v4());
        {
            let mut shinkai_db = self.db.lock().await;
            match shinkai_db.create_new_job(job_id.clone(), agent_id.clone(), job_creation.scope) {
                Ok(_) => (),
                Err(err) => return Err(JobManagerError::ShinkaiDB(err)),
            };

            match shinkai_db.get_job(&job_id) {
                Ok(job) => {
                    std::mem::drop(shinkai_db); // require to avoid deadlock
                    self.jobs.lock().await.insert(job_id.clone(), Box::new(job));
                    let mut agent_found = None;
                    for agent in &self.agents {
                        let locked_agent = agent.lock().await;
                        if &locked_agent.id == agent_id {
                            agent_found = Some(agent.clone());
                            break;
                        }
                    }

                    if agent_found.is_none() {
                        let identity_manager = self.identity_manager.lock().await;
                        if let Some(serialized_agent) = identity_manager.search_local_agent(&agent_id).await {
                            let agent = Agent::from_serialized_agent(serialized_agent, self.job_manager_sender.clone());
                            agent_found = Some(Arc::new(Mutex::new(agent)));
                            self.agents.push(agent_found.clone().unwrap());
                        }
                    }

                    let job_id_to_return = match agent_found {
                        Some(_) => Ok(job_id.clone()),
                        None => Err(anyhow::Error::new(JobManagerError::AgentNotFound)),
                    };

                    job_id_to_return.map_err(|_| JobManagerError::AgentNotFound)
                }
                Err(err) => {
                    return Err(JobManagerError::ShinkaiDB(err));
                }
            }
        }
    }

    /// Processes a job message and starts the decision phase
    pub async fn handle_job_message_schema(
        &mut self,
        message: ShinkaiMessage,
        job_message: JobMessage,
    ) -> Result<String, JobManagerError> {
        if let Some(job) = self.jobs.lock().await.get(&job_message.job_id) {
            let job = job.clone();
            let mut shinkai_db = self.db.lock().await;
            println!("handle_job_message_schema> job_message: {:?}", job_message);
            shinkai_db.add_message_to_job_inbox(&job_message.job_id.clone(), &message)?;
            shinkai_db.add_step_history(job.job_id().to_string(), job_message.content.clone())?;

            //
            // Todo: Implement unprocessed messages logic
            // If current unprocessed message count >= 1, then simply add unprocessed message and return success.
            // However if unprocessed message count  == 0, then:
            // 0. You add the unprocessed message to the list in the DB
            // 1. Start a while loop where every time you fetch the unprocessed messages for the job from the DB and check if there's >= 1
            // 2. You read the first/front unprocessed message (not pop from the back)
            // 3. You start analysis phase to generate the execution plan.
            // 4. You then take the execution plan and process the execution phase.
            // 5. Once execution phase succeeds, you then delete the message from the unprocessed list in the DB
            //    and take the result and append it both to the Job inbox and step history
            // 6. As we're in a while loop, go back to 1, meaning any new unprocessed messages added while the step was happening are now processed sequentially

            //
            // let current_unprocessed_message_count = ...
            shinkai_db.add_to_unprocessed_messages_list(job.job_id().to_string(), job_message.content.clone())?;

            std::mem::drop(shinkai_db); // require to avoid deadlock

            let _ = self.decision_phase(&**job).await?;
            return Ok(job_message.job_id.clone());
        } else {
            return Err(JobManagerError::JobNotFound);
        }
    }

    /// Adds pre-message to job inbox
    pub async fn handle_pre_message_schema(
        &mut self,
        pre_message: JobPreMessage,
        job_id: String,
        shinkai_message: ShinkaiMessage,
    ) -> Result<String, JobManagerError> {
        println!("handle_pre_message_schema> pre_message: {:?}", pre_message);

        self.db
            .lock()
            .await
            .add_message_to_job_inbox(job_id.as_str(), &shinkai_message)?;
        Ok(String::new())
    }

    pub async fn process_job_message(&mut self, message: ShinkaiMessage) -> Result<String, JobManagerError> {
        match message.clone().body {
            MessageBody::Unencrypted(body) => {
                match body.message_data {
                    MessageData::Unencrypted(data) => {
                        let message_type = data.message_content_schema;
                        match message_type {
                            MessageSchemaType::JobCreationSchema => {
                                let agent_name =
                                    ShinkaiName::from_shinkai_message_using_recipient_subidentity(&message)?;
                                let agent_id = agent_name.get_agent_name().ok_or(JobManagerError::AgentNotFound)?;
                                let job_creation: JobCreationInfo = serde_json::from_str(&data.message_raw_content)
                                    .map_err(|_| JobManagerError::ContentParseFailed)?;
                                self.handle_job_creation_schema(job_creation, &agent_id).await
                            }
                            MessageSchemaType::JobMessageSchema => {
                                let job_message: JobMessage = serde_json::from_str(&data.message_raw_content)
                                    .map_err(|_| JobManagerError::ContentParseFailed)?;
                                self.handle_job_message_schema(message, job_message).await
                            }
                            MessageSchemaType::PreMessageSchema => {
                                let pre_message: JobPreMessage = serde_json::from_str(&data.message_raw_content)
                                    .map_err(|_| JobManagerError::ContentParseFailed)?;
                                // TODO: we should be able to extract the job_id from the inbox
                                self.handle_pre_message_schema(pre_message, "".to_string(), message)
                                    .await
                            }
                            _ => {
                                // Handle Empty message type if needed, or return an error if it's not a valid job message
                                Err(JobManagerError::NotAJobMessage)
                            }
                        }
                    }
                    _ => Err(JobManagerError::NotAJobMessage),
                }
            }
            _ => Err(JobManagerError::NotAJobMessage),
        }
    }

    // When a new message is supplied to the job, the decision phase of the new step begins running
    // (with its existing step history as context) which triggers calling the Agent's LLM.
    async fn decision_phase(&self, job: &dyn JobLike) -> Result<(), Box<dyn Error>> {
        // Fetch the job
        let job_id = job.job_id().to_string();
        let full_job = { self.db.lock().await.get_job(&job_id).unwrap() };

        // Fetch context, if this is the first message of the job (new job just created), prefill step history with the default initial prompt
        let mut context = full_job.step_history.clone();
        // if context.len() == 1 {
        //     context.insert(0, JOB_INIT_PROMPT.clone());
        //     self.db
        //         .lock()
        //         .await
        //         .add_step_history(job_id.clone(), JOB_INIT_PROMPT.clone())?;
        // }

        let last_message = context.pop().ok_or(JobManagerError::ContentParseFailed)?.clone();

        // Acquire Agent
        let agent_id = full_job.parent_agent_id.clone();
        let mut agent_found = None;
        for agent in &self.agents {
            let locked_agent = agent.lock().await;
            if locked_agent.id == agent_id {
                agent_found = Some(agent.clone());
                break;
            }
        }

        match agent_found {
            Some(agent) => self.decision_iteration(full_job, context, last_message, agent).await,
            None => Err(Box::new(JobManagerError::AgentNotFound)),
        }
    }

    async fn decision_iteration(
        &self,
        job: Job,
        mut context: Vec<String>,
        last_message: String,
        agent: Arc<Mutex<Agent>>,
    ) -> Result<(), Box<dyn Error>> {
        // Append current time as ISO8601 to step history
        let time_with_comment = format!("{}: {}", "Current datetime ", Utc::now().to_rfc3339());
        context.push(time_with_comment);
        println!("decision_iteration> context: {:?}", context);
        println!("decision_iteration> last message: {:?}", last_message);

        // Execute LLM inferencing
        let response = tokio::spawn(async move {
            let mut agent = agent.lock().await;
            agent.execute(last_message, context, job.job_id().to_string()).await;
        })
        .await?;
        println!("decision_iteration> response: {:?}", response);

        // TODO: update this fn so it allows for recursion
        // let is_valid = self.is_decision_phase_output_valid().await;
        // if is_valid == false {
        //     self.decision_iteration(job, context, last_message, agent).await?;
        // }

        // The expected output from the LLM is one or more `Premessage`s (a message that potentially
        // still has computation that needs to be performed via tools to fill out its contents).
        // If the output from the LLM does not fit the expected structure, then the LLM is queried again
        // with the exact same inputs until a valid output is provided (potentially supplying extra text
        // each time to the LLM clarifying the previous result was invalid with an example/error message).

        // Make sure the output is valid
        // If not valid, keep calling the LLM until a valid output is produced
        // Return the output
        Ok(())
    }

    async fn is_decision_phase_output_valid(&self) -> bool {
        // Check if the output is valid
        // If not valid, return false
        // If valid, return true
        unimplemented!()
    }

    async fn execution_phase(&self, pre_messages: Vec<JobPreMessage>) -> Result<Vec<ShinkaiMessage>, Box<dyn Error>> {
        // For each Premessage:
        // 1. Call the necessary tools to fill out the contents
        // 2. Convert the Premessage into a Message
        // Return the list of Messages
        unimplemented!()
    }
}
