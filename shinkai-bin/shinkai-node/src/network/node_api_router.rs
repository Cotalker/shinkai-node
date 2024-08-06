use super::node::NodeCommand;
use super::v1_api::api_v1_router::v1_routes;
use async_channel::Sender;
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::json;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::shinkai_log;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::ShinkaiLogLevel;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::ShinkaiLogOption;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use warp::Filter;

#[derive(serde::Serialize, Debug, Clone)]
pub struct SendResponseBodyData {
    pub message_id: String,
    pub parent_message_id: Option<String>,
    pub inbox: String,
    pub scheduled_time: String,
}

#[derive(serde::Serialize, Debug, Clone)]
pub struct SendResponseBody {
    pub status: String,
    pub message: String,
    pub data: Option<SendResponseBodyData>,
}

#[derive(serde::Serialize)]
pub struct GetPublicKeysResponse {
    pub signature_public_key: String,
    pub encryption_public_key: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct APIError {
    pub code: u16,
    pub error: String,
    pub message: String,
}

impl APIError {
    pub fn new(code: StatusCode, error: &str, message: &str) -> Self {
        Self {
            code: code.as_u16(),
            error: error.to_string(),
            message: message.to_string(),
        }
    }
}

impl From<&str> for APIError {
    fn from(error: &str) -> Self {
        APIError {
            code: StatusCode::BAD_REQUEST.as_u16(),
            error: "Bad Request".to_string(),
            message: error.to_string(),
        }
    }
}

impl From<async_channel::SendError<NodeCommand>> for APIError {
    fn from(error: async_channel::SendError<NodeCommand>) -> Self {
        APIError {
            code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            error: "Internal Server Error".to_string(),
            message: format!("Failed with error: {}", error),
        }
    }
}

impl From<String> for APIError {
    fn from(error: String) -> Self {
        APIError {
            code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            error: "Internal Server Error".to_string(),
            message: error,
        }
    }
}

impl warp::reject::Reject for APIError {}

pub async fn run_api(
    node_commands_sender: Sender<NodeCommand>,
    address: SocketAddr,
    node_name: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    shinkai_log(
        ShinkaiLogOption::Api,
        ShinkaiLogLevel::Info,
        &format!("Starting Node API server at: {}", &address),
    );

    let log = warp::log::custom(|info| {
        shinkai_log(
            ShinkaiLogOption::Api,
            ShinkaiLogLevel::Debug,
            &format!(
                "ip: {:?}, method: {:?}, path: {:?}, status: {:?}, elapsed: {:?}",
                info.remote_addr(),
                info.method(),
                info.path(),
                info.status(),
                info.elapsed(),
            ),
        );
    });

    let cors = warp::cors() // build the CORS filter
        .allow_any_origin() // allow requests from any origin
        .allow_methods(vec!["GET", "POST", "OPTIONS"]) // allow GET, POST, and OPTIONS methods
        .allow_headers(vec!["Content-Type", "Authorization"]); // allow the Content-Type and Authorization headers

    let routes = v1_routes(node_commands_sender.clone(), node_name.clone())
        .recover(handle_rejection)
        .with(log)
        .with(cors);

    // Attempt to bind to the address before serving
    let try_bind = TcpListener::bind(&address).await;

    match try_bind {
        Ok(_) => {
            drop(try_bind);
            warp::serve(routes).run(address).await;
            Ok(())
        }
        Err(e) => {
            // If binding fails, return an error
            Err(Box::new(e))
        }
    }
}

pub async fn handle_node_command<T, U, V>(
    node_commands_sender: Sender<NodeCommand>,
    message: V,
    command: T,
) -> Result<impl warp::Reply, warp::reject::Rejection>
where
    T: FnOnce(Sender<NodeCommand>, V, Sender<Result<U, APIError>>) -> NodeCommand,
    U: Serialize,
    V: Serialize,
{
    let (res_sender, res_receiver) = async_channel::bounded(1);
    node_commands_sender
        .clone()
        .send(command(node_commands_sender, message, res_sender))
        .await
        .map_err(|_| warp::reject::reject())?;
    let result = res_receiver.recv().await.map_err(|_| warp::reject::reject())?;

    match result {
        Ok(message) => Ok(warp::reply::with_status(
            warp::reply::json(&json!({"status": "success", "data": message})),
            StatusCode::OK,
        )),
        Err(error) => Ok(warp::reply::with_status(
            warp::reply::json(&json!({"status": "error", "error": error.message})),
            StatusCode::from_u16(error.code).unwrap(),
        )),
    }
}

async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, warp::Rejection> {
    if let Some(api_error) = err.find::<APIError>() {
        let json = warp::reply::json(api_error);
        Ok(warp::reply::with_status(
            json,
            StatusCode::from_u16(api_error.code).unwrap(),
        ))
    } else if err.is_not_found() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::NOT_FOUND,
            "Not Found",
            "Please check your URL.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::NOT_FOUND))
    } else if err.find::<warp::filters::body::BodyDeserializeError>().is_some() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::BAD_REQUEST,
            "Invalid Body",
            "Please check your JSON body.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::BAD_REQUEST))
    } else if err.find::<warp::reject::MethodNotAllowed>().is_some() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "Method Not Allowed",
            "Please check your request method.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::METHOD_NOT_ALLOWED))
    } else if err.find::<warp::reject::PayloadTooLarge>().is_some() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Payload Too Large",
            "The request payload is too large.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::PAYLOAD_TOO_LARGE))
    } else if err.find::<warp::reject::InvalidQuery>().is_some() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::BAD_REQUEST,
            "Invalid Query",
            "The request query string is invalid.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::BAD_REQUEST))
    } else {
        // Unexpected error, we don't want to expose anything to the user.
        let json = warp::reply::json(&APIError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error",
            "An unexpected error occurred. Please try again.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::INTERNAL_SERVER_ERROR))
    }
}
