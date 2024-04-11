use super::{
    node_api::APIError,
    node_error::NodeError,
    Node,
};
use async_channel::Sender;
use reqwest::StatusCode;

impl Node {
    pub async fn api_private_devops_cron_list(&self, res: Sender<Result<String, APIError>>) -> Result<(), NodeError> {
        // Call the get_all_cron_tasks_from_all_profiles function
        match self.db.get_all_cron_tasks_from_all_profiles(self.node_name.clone()) {
            Ok(tasks) => {
                // If everything went well, send the tasks back as a JSON string
                let tasks_json = serde_json::to_string(&tasks).unwrap();
                let _ = res.send(Ok(tasks_json)).await;
                Ok(())
            }
            Err(err) => {
                // If there was an error, send the error message
                let api_error = APIError {
                    code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                    error: "Internal Server Error".to_string(),
                    message: format!("{}", err),
                };
                let _ = res.send(Err(api_error)).await;
                return Ok(());
            }
        }
    }
}
