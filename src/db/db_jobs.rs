use super::{db::Topic, db_errors::ShinkaiMessageDBError, ShinkaiMessageDB};
use crate::managers::identity_manager::{Identity, IdentityType};
use crate::managers::job_manager::{Job, JobLike};
use crate::schemas::inbox_name::InboxName;
use crate::schemas::job_schemas::JobScope;
use crate::shinkai_message::encryption::{encryption_public_key_to_string, encryption_public_key_to_string_ref};
use crate::shinkai_message::shinkai_message_handler::ShinkaiMessageHandler;
use crate::shinkai_message::signatures::{signature_public_key_to_string, signature_public_key_to_string_ref};
use crate::shinkai_message::{encryption::string_to_encryption_public_key, signatures::string_to_signature_public_key};
use ed25519_dalek::{PublicKey as SignaturePublicKey, SecretKey as SignatureStaticKey};
use rand::RngCore;
use rocksdb::{Error, IteratorMode, Options, WriteBatch};
use x25519_dalek::{PublicKey as EncryptionPublicKey, StaticSecret as EncryptionStaticKey};

enum JobInfo {
    IsFinished,
    DatetimeCreated,
    ParentAgentId,
    ConversationInboxName,
}

impl JobInfo {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "is_finished" => Some(Self::IsFinished),
            "datetime_created" => Some(Self::DatetimeCreated),
            "parent_agent_id" => Some(Self::ParentAgentId),
            "conversation_inbox_name" => Some(Self::ConversationInboxName),
            _ => None,
        }
    }

    fn to_str(&self) -> &'static str {
        match self {
            Self::IsFinished => "is_finished",
            Self::DatetimeCreated => "datetime_created",
            Self::ParentAgentId => "parent_agent_id",
            Self::ConversationInboxName => "conversation_inbox_name",
        }
    }
}

impl ShinkaiMessageDB {
    pub fn create_new_job(
        &mut self,
        job_id: String,
        agent_id: String,
        scope: JobScope,
    ) -> Result<(), ShinkaiMessageDBError> {
        println!("Creating job with id: {}", job_id);

        // Create Options for ColumnFamily
        let mut cf_opts = Options::default();
        cf_opts.create_if_missing(true);
        cf_opts.create_missing_column_families(true);

        // Create ColumnFamilyDescriptors for inbox and permission lists
        let cf_job_id_scope_name = format!("{}_scope", &job_id); // keyed by name and value link to bucket or document
        let cf_job_id_step_history_name = format!("{}_step_history", &job_id); // keyed by time (do I need composite? probably)
        let cf_agent_id_name = format!("agentid_{}", &agent_id);
        let cf_job_id_name = format!("jobtopic_{}", &job_id);
        let cf_conversation_inbox_name = format!("conversation_inbox_{}::|::|::false", &job_id);

        // Check that the profile name exists in ProfilesIdentityKey, ProfilesEncryptionKey and ProfilesIdentityType
        if self.db.cf_handle(&cf_job_id_scope_name).is_some()
            || self.db.cf_handle(&cf_job_id_step_history_name).is_some()
            || self.db.cf_handle(&cf_job_id_name).is_some()
            || self.db.cf_handle(&cf_conversation_inbox_name).is_some()
        {
            return Err(ShinkaiMessageDBError::ProfileNameAlreadyExists);
        }

        if self.db.cf_handle(&cf_agent_id_name).is_none() {
            self.db.create_cf(&cf_agent_id_name, &cf_opts)?;
        }

        self.db.create_cf(&cf_job_id_name, &cf_opts)?;
        self.db.create_cf(&cf_job_id_scope_name, &cf_opts)?;
        self.db.create_cf(&cf_job_id_step_history_name, &cf_opts)?;
        self.db.create_cf(&cf_conversation_inbox_name, &cf_opts)?;

        // Start a write batch
        let mut batch = WriteBatch::default();

        // Generate time now used as a key. it should be safe because it's generated here so it shouldn't be duplicated (presumably)
        let current_time = ShinkaiMessageHandler::generate_time_now();
        let scope_bytes = scope.to_bytes()?;

        let cf_job_id = self.db.cf_handle(&cf_job_id_name).unwrap();
        let cf_agent_id = self.db.cf_handle(&cf_agent_id_name).unwrap();
        let cf_job_id_scope = self.db.cf_handle(&cf_job_id_scope_name).unwrap();

        batch.put_cf(cf_agent_id, current_time.as_bytes(), job_id.as_bytes());
        batch.put_cf(cf_job_id_scope, job_id.as_bytes(), &scope_bytes);
        batch.put_cf(cf_job_id, JobInfo::IsFinished.to_str().as_bytes(), b"false");
        batch.put_cf(
            cf_job_id,
            JobInfo::DatetimeCreated.to_str().as_bytes(),
            current_time.as_bytes(),
        );
        batch.put_cf(
            cf_job_id,
            JobInfo::ParentAgentId.to_str().as_bytes(),
            agent_id.as_bytes(),
        );
        batch.put_cf(
            cf_job_id,
            JobInfo::ConversationInboxName.to_str().as_bytes(),
            cf_conversation_inbox_name.as_bytes(),
        );

        let cf_jobs = self
            .db
            .cf_handle(Topic::AllJobsTimeKeyed.as_str())
            .expect("to be able to access Topic::AllJobsTimeKeyed");
        batch.put_cf(cf_jobs, &current_time, &job_id);

        self.db.write(batch)?;

        Ok(())
    }

    fn get_job_data(
        &self,
        job_id: &str,
        fetch_step_history: bool,
    ) -> Result<(JobScope, bool, String, String, InboxName, Option<Vec<String>>), ShinkaiMessageDBError> {
        // Initialize the column family names
        let cf_job_id_name = format!("jobtopic_{}", job_id);
        let cf_job_id_scope_name = format!("{}_scope", job_id);
        let cf_job_id_step_history_name = format!("{}_step_history", job_id);

        // Get the column family handles
        let cf_job_id_scope = self
            .db
            .cf_handle(&cf_job_id_scope_name)
            .ok_or(ShinkaiMessageDBError::ProfileNameNonExistent)?;
        let cf_job_id = self
            .db
            .cf_handle(&cf_job_id_name)
            .ok_or(ShinkaiMessageDBError::ProfileNameNonExistent)?;

        // Get the scope
        let scope_value = self.db.get_cf(cf_job_id_scope, job_id)?;
        let scope = match scope_value {
            Some(scope_bytes) => JobScope::from_bytes(&scope_bytes)?,
            None => return Err(ShinkaiMessageDBError::DataNotFound),
        };

        // Get the job is_finished status
        let is_finished_value = self.db.get_cf(cf_job_id, JobInfo::IsFinished.to_str().as_bytes())?;
        let is_finished = match is_finished_value {
            Some(bytes) => std::str::from_utf8(&bytes)?.to_string() == "true",
            None => return Err(ShinkaiMessageDBError::DataNotFound),
        };

        // Get the datetime_created and parent_agent_id from step history
        let cf_job_id_step_history = self
            .db
            .cf_handle(&cf_job_id_step_history_name)
            .ok_or(ShinkaiMessageDBError::ProfileNameNonExistent)?;

        let datetime_created_value = self
            .db
            .get_cf(cf_job_id, JobInfo::DatetimeCreated.to_str().as_bytes())?;
        let datetime_created = match datetime_created_value {
            Some(bytes) => std::str::from_utf8(&bytes)?.to_string(),
            None => return Err(ShinkaiMessageDBError::DataNotFound),
        };

        let parent_agent_id_value = self.db.get_cf(cf_job_id, JobInfo::ParentAgentId.to_str().as_bytes())?;
        let parent_agent_id = match parent_agent_id_value {
            Some(bytes) => std::str::from_utf8(&bytes)?.to_string(),
            None => return Err(ShinkaiMessageDBError::DataNotFound),
        };

        // Get the conversation_inbox and step_history
        let mut conversation_inbox: Option<InboxName> = None;
        let mut step_history: Option<Vec<String>> = if fetch_step_history { Some(Vec::new()) } else { None };

        // Get the conversation_inbox
        let conversation_inbox_value = self.db.get_cf(
            cf_job_id,
            JobInfo::ConversationInboxName.to_str().as_bytes(),
        )?;
        match conversation_inbox_value {
            Some(value) => {
                let inbox_name =
                    String::from_utf8(value.to_vec()).map_err(|_| ShinkaiMessageDBError::DataConversionError)?;
                conversation_inbox = Some(InboxName::new(inbox_name)?);
            }
            None => {
                return Err(ShinkaiMessageDBError::InboxNotFound)
            }
        }

        // Get the step_history
        if let Some(ref mut step_history) = step_history {
            let iter = self.db.iterator_cf(cf_job_id_step_history, IteratorMode::Start);
            for item in iter {
                match item {
                    Ok((_key, value)) => {
                        let step = String::from_utf8(value.to_vec())
                            .map_err(|_| ShinkaiMessageDBError::DataConversionError)?;
                        step_history.push(step);
                    }
                    Err(e) => return Err(ShinkaiMessageDBError::RocksDBError(e)),
                }
            }
        }

        Ok((
            scope,
            is_finished,
            datetime_created,
            parent_agent_id,
            conversation_inbox.unwrap(),
            step_history,
        ))
    }

    pub fn get_job(&self, job_id: &str) -> Result<Job, ShinkaiMessageDBError> {
        let (scope, is_finished, datetime_created, parent_agent_id, conversation_inbox, step_history) =
            self.get_job_data(job_id, true)?;

        // Construct the job
        let job = Job {
            job_id: job_id.to_string(),
            datetime_created,
            is_finished,
            parent_agent_id,
            scope,
            conversation_inbox_name: conversation_inbox,
            step_history: step_history.unwrap_or_else(Vec::new),
        };

        Ok(job)
    }

    pub fn get_job_like(&self, job_id: &str) -> Result<Box<dyn JobLike>, ShinkaiMessageDBError> {
        let (scope, is_finished, datetime_created, parent_agent_id, conversation_inbox, _) =
            self.get_job_data(job_id, false)?;

        // Construct the job
        let job = Job {
            job_id: job_id.to_string(),
            datetime_created,
            is_finished,
            parent_agent_id,
            scope,
            conversation_inbox_name: conversation_inbox,
            step_history: Vec::new(), // Empty step history for JobLike
        };

        Ok(Box::new(job))
    }

    pub fn get_all_jobs(&self) -> Result<Vec<Box<dyn JobLike>>, ShinkaiMessageDBError> {
        let cf_handle = self
            .db
            .cf_handle(Topic::AllJobsTimeKeyed.as_str())
            .ok_or(ShinkaiMessageDBError::ProfileNameNonExistent)?;
        let mut jobs = Vec::new();
        let iter = self.db.iterator_cf(cf_handle, IteratorMode::Start);
        for item in iter {
            match item {
                Ok((_key, value)) => {
                    let job_id =
                        String::from_utf8(value.to_vec()).map_err(|_| ShinkaiMessageDBError::DataConversionError)?;
                    let job = self.get_job_like(&job_id)?;
                    jobs.push(job);
                }
                Err(e) => return Err(ShinkaiMessageDBError::RocksDBError(e)),
            }
        }
        Ok(jobs)
    }

    pub fn get_agent_jobs(&self, agent_id: String) -> Result<Vec<Box<dyn JobLike>>, ShinkaiMessageDBError> {
        let cf_name = format!("agentid_{}", &agent_id);
        let cf_handle = self
            .db
            .cf_handle(&cf_name)
            .ok_or(ShinkaiMessageDBError::ProfileNameNonExistent)?;
        let mut jobs = Vec::new();
        let iter = self.db.iterator_cf(cf_handle, IteratorMode::Start);
        for item in iter {
            match item {
                Ok((_, value)) => {
                    let job_id =
                        String::from_utf8(value.to_vec()).map_err(|_| ShinkaiMessageDBError::DataConversionError)?;
                    let job = self.get_job_like(&job_id)?;
                    jobs.push(job);
                }
                Err(e) => return Err(ShinkaiMessageDBError::RocksDBError(e)),
            }
        }
        // // Sorting in reverse to get the jobs from most recent to oldest
        // jobs.sort_unstable_by(|a, b| b.datetime_created.cmp(&a.datetime_created));
        Ok(jobs)
    }

    pub fn update_job_to_finished(&self, job_id: String) -> Result<(), ShinkaiMessageDBError> {
        let cf_name = format!("jobtopic_{}", &job_id);
        let cf_handle = self
            .db
            .cf_handle(&cf_name)
            .ok_or(ShinkaiMessageDBError::ProfileNameNonExistent)?;
        let mut batch = WriteBatch::default();
        batch.put_cf(cf_handle, JobInfo::IsFinished.to_str().as_bytes(), b"true");
        self.db.write(batch)?;
        Ok(())
    }

    pub fn update_step_history(&self, job_id: String, step: String) -> Result<(), ShinkaiMessageDBError> {
        let cf_name = format!("{}_step_history", &job_id);
        let cf_handle = self
            .db
            .cf_handle(&cf_name)
            .ok_or(ShinkaiMessageDBError::ProfileNameNonExistent)?;
        let current_time = ShinkaiMessageHandler::generate_time_now();
        self.db.put_cf(cf_handle, current_time.as_bytes(), step.as_bytes())?;
        Ok(())
    }
}
