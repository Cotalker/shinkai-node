use crate::db::db_errors::ShinkaiDBError;
use crate::db::ShinkaiDB;
use crate::llm_provider::job_manager::JobManager;
use async_channel::{Receiver, Sender};
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_sheet::sheet::{ColumnDefinition, Sheet, SheetUpdate};
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct SheetManager {
    pub sheets: HashMap<String, (Sheet, Sender<SheetUpdate>)>,
    pub db: Weak<ShinkaiDB>,
    pub job_manager: Arc<Mutex<JobManager>>,
    pub user_profile: ShinkaiName,
}

impl SheetManager {
    pub async fn new(
        db: Weak<ShinkaiDB>,
        job_manager: Arc<Mutex<JobManager>>,
        node_name: ShinkaiName,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Only works for main right now
        let user_profile = ShinkaiName::from_node_and_profile_names(node_name.node_name, "main".to_string())?;
        let db_strong = db
            .upgrade()
            .ok_or(ShinkaiDBError::SomeError("Couldn't convert to strong db".to_string()))?;

        let sheets_vec = db_strong.list_all_sheets_for_user(&user_profile)?;
        let sheets = sheets_vec
            .into_iter()
            .map(|mut sheet| {
                let (sender, receiver) = async_channel::unbounded();
                sheet.set_update_sender(sender.clone());
                // Start a task to handle updates
                tokio::spawn(Self::handle_updates(receiver));
                (sheet.uuid.clone(), (sheet, sender))
            })
            .collect();

        Ok(Self {
            sheets,
            db,
            job_manager,
            user_profile,
        })
    }

    pub fn create_empty_sheet(&mut self) -> Result<(), ShinkaiDBError> {
        let sheet = Sheet::new();
        let sheet_id = sheet.uuid.clone();
        let (sender, _receiver) = async_channel::unbounded();
        let mut sheet_clone = sheet.clone();
        sheet_clone.set_update_sender(sender.clone());

        self.sheets.insert(sheet_id.clone(), (sheet_clone, sender));

        // Add the sheet to the database
        let db_strong = self
            .db
            .upgrade()
            .ok_or(ShinkaiDBError::SomeError("Couldn't convert to strong db".to_string()))?;
        db_strong.save_sheet(sheet, self.user_profile.clone())?;

        Ok(())
    }

    pub fn add_sheet(&mut self, sheet: Sheet) -> Result<(), ShinkaiDBError> {
        let (sender, _receiver) = async_channel::unbounded();
        let sheet_id = sheet.uuid.clone();
        let mut sheet_clone = sheet.clone();
        sheet_clone.set_update_sender(sender.clone());

        self.sheets.insert(sheet_id, (sheet_clone, sender));

        // Add the sheet to the database
        let db_strong = self
            .db
            .upgrade()
            .ok_or(ShinkaiDBError::SomeError("Couldn't convert to strong db".to_string()))?;
        db_strong.save_sheet(sheet, self.user_profile.clone())?;

        Ok(())
    }

    pub fn remove_sheet(&mut self, sheet_id: &str) -> Result<(), ShinkaiDBError> {
        // Remove the sheet from the HashMap
        if self.sheets.remove(sheet_id).is_none() {
            return Err(ShinkaiDBError::SomeError("Sheet ID not found".to_string()));
        }

        // Remove the sheet from the database
        let db_strong = self
            .db
            .upgrade()
            .ok_or(ShinkaiDBError::SomeError("Couldn't convert to strong db".to_string()))?;
        db_strong.remove_sheet(sheet_id, &self.user_profile)?;

        Ok(())
    }

    pub async fn set_column(&mut self, sheet_id: &str, column: ColumnDefinition) -> Result<(), String> {
        let (sheet, _) = self.sheets.get_mut(sheet_id).ok_or("Sheet ID not found")?;
        let jobs = sheet.set_column(column.clone()).await;
        // TODO: add cb to jobs and send them

        // Update the sheet in the database
        let db_strong = self.db.upgrade().ok_or("Couldn't convert to strong db".to_string())?;
        db_strong
            .save_sheet(sheet.clone(), self.user_profile.clone())
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn remove_column(&mut self, sheet_id: &str, column_id: usize) -> Result<(), String> {
        let (sheet, _) = self.sheets.get_mut(sheet_id).ok_or("Sheet ID not found")?;
        let jobs = sheet.remove_column(column_id).await.map_err(|e| e.to_string())?;
        // TODO: add cb to jobs and send them

        // Update the sheet in the database
        let db_strong = self.db.upgrade().ok_or("Couldn't convert to strong db".to_string())?;
        db_strong
            .save_sheet(sheet.clone(), self.user_profile.clone())
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub async fn get_user_sheets(&self) -> Result<Vec<Sheet>, String> {
        let db_strong = self.db.upgrade().ok_or("Couldn't convert to strong db".to_string())?;
        db_strong
            .list_all_sheets_for_user(&self.user_profile)
            .map_err(|e| e.to_string())
    }

    pub async fn set_cell_value(
        &mut self,
        sheet_id: &str,
        row: usize,
        col: usize,
        value: String,
    ) -> Result<(), String> {
        let (sheet, _) = self.sheets.get_mut(sheet_id).ok_or("Sheet ID not found")?;
        let jobs = sheet
            .set_cell_value(row, col, value)
            .await?;
        // TODO: add cb to jobs and send them

        // Update the sheet in the database
        let db_strong = self.db.upgrade().ok_or("Couldn't convert to strong db".to_string())?;
        db_strong
            .save_sheet(sheet.clone(), self.user_profile.clone())
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn set_update_sender(&mut self, id: &str, sender: Sender<SheetUpdate>) -> Result<(), String> {
        if let Some((sheet, _)) = self.sheets.get_mut(id) {
            sheet.set_update_sender(sender);
            Ok(())
        } else {
            Err("Sheet ID not found".to_string())
        }
    }

    // TODO: is this necessary?
    // pub async fn initiate_workflow_job(
    //     &mut self,
    //     sheet_id: &str,
    //     row: usize,
    //     col: usize,
    //     workflow: &Workflow,
    //     input_columns: &[usize],
    //     llm_provider_name: &str,
    // ) -> Result<(), String> {
    //     let (sheet, _) = self.sheets.get_mut(sheet_id).ok_or("Sheet ID not found")?;
    //     let job = sheet.initiate_workflow_job(
    //         row,
    //         col,
    //         workflow,
    //         input_columns,
    //         llm_provider_name,
    //         self.workflow_job_creator.clone(),
    //     ).await;

    //     let job_message = JobMessage {
    //         job_id: job.id().to_string(),
    //         content: job.prompt().to_string(),
    //         files_inbox: "".to_string(),
    //         parent: None,
    //         workflow_code: None,
    //         workflow_name: None,
    //     };

    //     let profile = ShinkaiName::new("@@node1.shinkai/main".to_string()).unwrap();
    //     let mut job_manager = self.job_manager.lock().await;
    //     job_manager
    //         .add_job_message_to_job_queue(&job_message, &profile)
    //         .await
    //         .map_err(|e| e.to_string())?;

    //     Ok(())
    // }

    async fn handle_updates(receiver: Receiver<SheetUpdate>) {
        while let Ok(update) = receiver.recv().await {
            // Handle the update (e.g., log it, process it, etc.)
            // TODO: check from which sheet the update came from
            println!("Received update: {:?}", update);
        }
    }
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct WorkflowSheetJob {
//     id: String,
//     cell_id: CellId,
//     prompt: String,
//     dependencies: Vec<CellId>,
//     status: JobStatus,
//     created_at: DateTime<Utc>,
//     updated_at: DateTime<Utc>,
//     result: Option<String>,
// }

// impl SheetJob for WorkflowSheetJob {
//     fn id(&self) -> &str { &self.id }
//     fn cell_id(&self) -> &CellId { &self.cell_id }
//     fn prompt(&self) -> &str { &self.prompt }
//     fn dependencies(&self) -> &[CellId] { &self.dependencies }
//     fn status(&self) -> JobStatus { self.status.clone() }
//     fn created_at(&self) -> DateTime<Utc> { self.created_at }
//     fn updated_at(&self) -> DateTime<Utc> { self.updated_at }
//     fn result(&self) -> Option<&str> { self.result.as_deref() }
//     fn set_status(&mut self, status: JobStatus) {
//         self.status = status;
//         self.updated_at = Utc::now();
//     }
//     fn set_result(&mut self, result: String) {
//         self.result = Some(result);
//         self.updated_at = Utc::now();
//     }
// }

// impl WorkflowSheetJob {
//     pub fn new(id: String, cell_id: CellId, prompt: String, dependencies: Vec<CellId>) -> Self {
//         let now = Utc::now();
//         Self {
//             id,
//             cell_id,
//             prompt,
//             dependencies,
//             status: JobStatus::Pending,
//             created_at: now,
//             updated_at: now,
//             result: None,
//         }
//     }
// }
