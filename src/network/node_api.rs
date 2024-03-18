use super::node::NodeCommand;
use async_channel::Sender;
use futures::StreamExt;
use futures::TryFutureExt;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use shinkai_message_primitives::shinkai_message::shinkai_message::ShinkaiMessage;
use shinkai_message_primitives::shinkai_utils::encryption::encryption_public_key_to_string;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::shinkai_log;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::ShinkaiLogLevel;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::ShinkaiLogOption;
use shinkai_message_primitives::shinkai_utils::signatures::signature_public_key_to_string;
use std::net::SocketAddr;
use warp::Buf;
use warp::Filter;

#[derive(serde::Deserialize)]
struct NameToExternalProfileData {
    name: String,
}

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
struct GetPublicKeysResponse {
    signature_public_key: String,
    encryption_public_key: String,
}

#[derive(serde::Serialize)]
struct IdentityNameToExternalProfileDataResponse {
    signature_public_key: String,
    encryption_public_key: String,
}

#[derive(serde::Deserialize)]
struct ConnectBody {
    address: String,
    profile_name: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct APIError {
    pub code: u16,
    pub error: String,
    pub message: String,
}

impl APIError {
    fn new(code: StatusCode, error: &str, message: &str) -> Self {
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

pub async fn run_api(node_commands_sender: Sender<NodeCommand>, address: SocketAddr, node_name: String) {
    shinkai_log(
        ShinkaiLogOption::API,
        ShinkaiLogLevel::Info,
        &format!("Starting Node API server at: {}", &address),
    );

    let log = warp::log::custom(|info| {
        shinkai_log(
            ShinkaiLogOption::API,
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

    let ping_all = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "ping_all")
            .and(warp::post())
            .and_then(move || ping_all_handler(node_commands_sender.clone()))
    };

    // POST v1/send
    let send_msg = {
        let node_commands_sender = node_commands_sender.clone();
        warp::post()
            .and(warp::path("v1"))
            .and(warp::path("send"))
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| send_msg_handler(node_commands_sender.clone(), message))
    };

    // GET v1/get_peers
    let get_peers = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "get_peers")
            .and(warp::get())
            .and_then(move || get_peers_handler(node_commands_sender.clone()))
    };

    // POST v1/identity_name_to_external_profile_data
    let identity_name_to_external_profile_data = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "identity_name_to_external_profile_data")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |body: NameToExternalProfileData| {
                identity_name_to_external_profile_data_handler(node_commands_sender.clone(), body)
            })
    };

    // GET v1/get_public_keys
    let get_public_key = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "get_public_keys")
            .and(warp::get())
            .and_then(move || get_public_key_handler(node_commands_sender.clone()))
    };

    // POST v1/add_toolkit
    let add_toolkit = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "add_toolkit")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| add_toolkit_handler(node_commands_sender.clone(), message))
    };

    // POST v1/vec_fs/retrieve_path_simplified_json
    let api_vec_fs_retrieve_path_simplified_json = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "retrieve_path_simplified_json")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_retrieve_path_simplified_json_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/retrieve_vector_search_simplified_json
    let api_vec_fs_retrieve_vector_search_simplified_json = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "retrieve_vector_search_simplified_json")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_retrieve_vector_search_simplified_json_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/search_items
    let api_vec_fs_search_items = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "search_items")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_search_item_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/create_folder
    let api_vec_fs_create_folder = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "create_folder")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_create_folder_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/move_folder
    let api_vec_fs_move_folder = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "move_folder")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_move_folder_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/copy_folder
    let api_vec_fs_copy_folder = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "copy_folder")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_copy_folder_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/remove_folder
    let api_vec_fs_remove_folder = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "remove_folder")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_remove_folder_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/move_item
    let api_vec_fs_move_item = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "move_item")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_move_item_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/copy_item
    let api_vec_fs_copy_item = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "copy_item")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_copy_item_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/remove_item
    let api_vec_fs_remove_item = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "remove_item")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_remove_item_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/retrieve_vector_resource
    let api_convert_files_and_save_to_folder = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "convert_files_and_save_to_folder")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_convert_files_and_save_to_folder_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/vec_fs/retrieve_vector_resource
    let api_vec_fs_retrieve_vector_resource = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "vec_fs" / "retrieve_vector_resource")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_vec_fs_retrieve_vector_resource_handler(node_commands_sender.clone(), message)
            })
    };

    // GET v1/shinkai_health
    let shinkai_health = {
        let node_name = node_name.clone();
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "shinkai_health")
            .and(warp::get())
            .and_then(move || shinkai_health_handler(node_commands_sender.clone(), node_name.clone()))
    };

    // TODO: Implement. Admin Only
    // // POST v1/last_messages?limit={number}&offset={key}
    // let get_last_messages = {
    //     let node_commands_sender = node_commands_sender.clone();
    //     warp::path!("v1" / "last_messages_from_inbox")
    //         .and(warp::post())
    //         .and(warp::body::json::<ShinkaiMessage>())
    //         .and_then(move |message: ShinkaiMessage| {
    //             get_last_messages_handler(node_commands_sender.clone(), message)
    //         })
    // };

    // POST v1/available_agents
    let available_agents = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "available_agents")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| available_agents_handler(node_commands_sender.clone(), message))
    };

    // POST v1/add_agent
    let add_agent = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "add_agent")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| add_agent_handler(node_commands_sender.clone(), message))
    };

    // POST v1/last_messages_from_inbox?limit={number}&offset={key}
    let get_last_messages_from_inbox = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "last_messages_from_inbox")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                get_last_messages_from_inbox_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/last_unread_messages?limit={number}&offset={key}
    let get_last_unread_messages = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "last_unread_messages_from_inbox")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                get_last_unread_messages_from_inbox_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/get_all_inboxes_for_profile_handler
    let get_all_inboxes_for_profile = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "get_all_inboxes_for_profile")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                get_all_inboxes_for_profile_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/get_all_smart_inboxes_for_profile_handler
    let get_all_smart_inboxes_for_profile = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "get_all_smart_inboxes_for_profile")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                get_all_smart_inboxes_for_profile_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/update_smart_inbox_name_handler
    let update_smart_inbox_name = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "update_smart_inbox_name")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                update_smart_inbox_name_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/create_job
    let create_job = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "create_job")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| create_job_handler(node_commands_sender.clone(), message))
    };

    // POST v1/job_message
    let job_message = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "job_message")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| job_message_handler(node_commands_sender.clone(), message))
    };

    // POST v1/get_filenames_for_file_inbox
    let get_filenames = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "get_filenames_for_file_inbox")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                get_filenames_message_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/mark_as_read_up_to
    let mark_as_read_up_to = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "mark_as_read_up_to")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| mark_as_read_up_to_handler(node_commands_sender.clone(), message))
    };

    // POST v1/create_registration_code
    let create_registration_code = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "create_registration_code")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                create_registration_code_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/use_registration_code
    let use_registration_code = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "use_registration_code")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                use_registration_code_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/change_nodes_name
    let change_nodes_name = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "change_nodes_name")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| change_nodes_name_handler(node_commands_sender.clone(), message))
    };

    // GET v1/get_all_subidentities
    let get_all_subidentities = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "get_all_subidentities")
            .and(warp::get())
            .and_then(move || get_all_subidentities_handler(node_commands_sender.clone()))
    };

    // POST v1/last_messages_from_inbox_with_branches
    let get_last_messages_from_inbox_with_branches = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "last_messages_from_inbox_with_branches")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                get_last_messages_from_inbox_with_branches_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/create_files_inbox_with_symmetric_key
    let create_files_inbox_with_symmetric_key = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "create_files_inbox_with_symmetric_key")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                create_files_inbox_with_symmetric_key_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/add_file_to_inbox_with_symmetric_key/{string1}/{string2}
    let add_file_to_inbox_with_symmetric_key = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "add_file_to_inbox_with_symmetric_key" / String / String)
            .and(warp::post())
            .and(warp::body::content_length_limit(1024 * 1024 * 200)) // 200MB
            .and(warp::multipart::form().max_length(1024 * 1024 * 200))
            .and_then(
                move |string1: String, string2: String, form: warp::multipart::FormData| {
                    handle_file_upload(node_commands_sender.clone(), string1, string2, form)
                },
            )
    };

    // POST v1/update_job_to_finished
    let update_job_to_finished = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "update_job_to_finished")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                update_job_to_finished_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/available_shared_items
    let api_available_shared_items = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "available_shared_items")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_subscription_available_shared_items_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/create_shareable_folder
    let api_create_shareable_folder = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "create_shareable_folder")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_subscription_create_shareable_folder_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/update_shareable_folder
    let api_update_shareable_folder = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "update_shareable_folder")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_subscription_update_shareable_folder_handler(node_commands_sender.clone(), message)
            })
    };

    // POST v1/unshare_folder
    let api_unshare_folder = {
        let node_commands_sender = node_commands_sender.clone();
        warp::path!("v1" / "unshare_folder")
            .and(warp::post())
            .and(warp::body::json::<ShinkaiMessage>())
            .and_then(move |message: ShinkaiMessage| {
                api_subscription_unshare_folder_handler(node_commands_sender.clone(), message)
            })
    };

    let cors = warp::cors() // build the CORS filter
        .allow_any_origin() // allow requests from any origin
        .allow_methods(vec!["GET", "POST", "OPTIONS"]) // allow GET, POST, and OPTIONS methods
        .allow_headers(vec!["Content-Type", "Authorization"]); // allow the Content-Type and Authorization headers

    let routes = ping_all
        .or(send_msg)
        .or(get_peers)
        .or(identity_name_to_external_profile_data)
        .or(get_public_key)
        .or(get_all_inboxes_for_profile)
        .or(get_all_smart_inboxes_for_profile)
        .or(update_smart_inbox_name)
        .or(available_agents)
        .or(add_agent)
        .or(get_last_messages_from_inbox)
        .or(get_last_unread_messages)
        .or(create_job)
        .or(job_message)
        .or(mark_as_read_up_to)
        .or(create_registration_code)
        .or(use_registration_code)
        .or(get_all_subidentities)
        .or(shinkai_health)
        .or(create_files_inbox_with_symmetric_key)
        .or(add_file_to_inbox_with_symmetric_key)
        .or(get_filenames)
        .or(update_job_to_finished)
        .or(add_toolkit)
        .or(change_nodes_name)
        .or(get_last_messages_from_inbox_with_branches)
        .or(api_vec_fs_retrieve_path_simplified_json)
        .or(api_vec_fs_retrieve_vector_search_simplified_json)
        .or(api_vec_fs_search_items)
        .or(api_vec_fs_create_folder)
        .or(api_vec_fs_move_item)
        .or(api_vec_fs_copy_item)
        .or(api_vec_fs_remove_item)
        .or(api_vec_fs_move_folder)
        .or(api_vec_fs_copy_folder)
        .or(api_vec_fs_remove_folder)
        .or(api_vec_fs_retrieve_vector_resource)
        .or(api_convert_files_and_save_to_folder)
        .or(api_available_shared_items)
        .or(api_create_shareable_folder)
        .or(api_update_shareable_folder)
        .or(api_unshare_folder)
        .recover(handle_rejection)
        .with(log)
        .with(cors);

    warp::serve(routes).run(address).await;
}

async fn handle_node_command<T, U>(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
    command: T,
) -> Result<impl warp::Reply, warp::reject::Rejection>
where
    T: FnOnce(Sender<NodeCommand>, ShinkaiMessage, Sender<Result<U, APIError>>) -> NodeCommand,
    U: Serialize,
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

async fn handle_node_command_without_message<T, U>(
    node_commands_sender: Sender<NodeCommand>,
    command: T,
) -> Result<Box<dyn warp::Reply>, warp::Rejection>
where
    T: FnOnce(Sender<NodeCommand>, Sender<Result<U, APIError>>) -> NodeCommand,
    U: Serialize,
{
    let (res_sender, res_receiver) = async_channel::bounded(1);
    node_commands_sender
        .clone()
        .send(command(node_commands_sender, res_sender))
        .await
        .map_err(|_| warp::reject::reject())?;
    let result = res_receiver.recv().await.map_err(|_| warp::reject::reject())?;

    match result {
        Ok(message) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&json!({"status": "success", "data": message})),
            StatusCode::OK,
        )) as Box<dyn warp::Reply>),
        Err(error) => Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&json!({"status": "error", "error": error})),
            StatusCode::from_u16(error.code).unwrap(),
        )) as Box<dyn warp::Reply>),
    }
}

async fn ping_all_handler(node_commands_sender: Sender<NodeCommand>) -> Result<impl warp::Reply, warp::Rejection> {
    match node_commands_sender.send(NodeCommand::PingAll).await {
        Ok(_) => Ok(warp::reply::json(&json!({
            "result": "Pinged all nodes successfully"
        }))),
        Err(_) => Ok(warp::reply::json(&json!({
            "error": "Error occurred while pinging all nodes"
        }))),
    }
}

async fn api_subscription_available_shared_items_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIAvailableSharedItems {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_subscription_create_shareable_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APICreateShareableFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_subscription_update_shareable_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIUpdateShareableFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_subscription_unshare_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIUnshareFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn add_toolkit_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIAddToolkit {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_retrieve_path_simplified_json_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSRetrievePathSimplifiedJson {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_retrieve_vector_search_simplified_json_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSRetrieveVectorSearchSimplifiedJson {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_search_item_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSSearchItems {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_delete_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSDeleteFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_delete_item_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSDeleteItem {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_create_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSCreateFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_move_item_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSMoveItem {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_copy_item_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSCopyItem {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_remove_item_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSDeleteItem {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_move_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSMoveFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_remove_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSDeleteFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_copy_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSCopyFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_vec_fs_retrieve_vector_resource_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIVecFSRetrieveVectorResource {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn api_convert_files_and_save_to_folder_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(
        node_commands_sender,
        message,
        |node_commands_sender, message, res_sender| NodeCommand::APIConvertFilesAndSaveToFolder {
            msg: message,
            res: res_sender,
        },
    )
    .await
}

async fn change_nodes_name_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIChangeNodesName {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn send_msg_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    let node_commands_sender = node_commands_sender.clone();
    let (res_send_msg_sender, res_send_msg_receiver): (
        async_channel::Sender<Result<SendResponseBodyData, APIError>>,
        async_channel::Receiver<Result<SendResponseBodyData, APIError>>,
    ) = async_channel::bounded(1);
    node_commands_sender
        .send(NodeCommand::SendOnionizedMessage {
            msg: message,
            res: res_send_msg_sender,
        })
        .await
        .map_err(|e| warp::reject::custom(APIError::from(e)))?;
    let send_result = res_send_msg_receiver
        .recv()
        .await
        .map_err(|e| warp::reject::custom(APIError::from(format!("{}", e))))?;
    match send_result {
        Ok(data) => {
            let response_body = SendResponseBody {
                status: "success".to_string(),
                message: "Message sent successfully".to_string(),
                data: Some(data),
            };
            Ok(warp::reply::json(&response_body))
        }
        Err(api_error) => Err(warp::reject::custom(api_error)),
    }
}

async fn get_peers_handler(node_commands_sender: Sender<NodeCommand>) -> Result<impl warp::Reply, warp::Rejection> {
    let node_commands_sender = node_commands_sender.clone();
    let (res_sender, res_receiver) = async_channel::bounded(1);
    node_commands_sender
        .send(NodeCommand::GetPeers(res_sender))
        .await
        .map_err(|_| warp::reject::reject())?; // Send the command to Node
    let peer_addresses = res_receiver.recv().await.unwrap();
    Ok(warp::reply::json(&peer_addresses))
}

async fn identity_name_to_external_profile_data_handler(
    node_commands_sender: Sender<NodeCommand>,
    body: NameToExternalProfileData,
) -> Result<impl warp::Reply, warp::Rejection> {
    let node_commands_sender = node_commands_sender.clone();
    let (res_sender, res_receiver) = async_channel::bounded(1);
    node_commands_sender
        .send(NodeCommand::IdentityNameToExternalProfileData {
            name: body.name,
            res: res_sender,
        })
        .await
        .map_err(|_| warp::reject::reject())?;
    let external_profile_data = res_receiver.recv().await.map_err(|_| warp::reject::reject())?;
    Ok(warp::reply::json(&IdentityNameToExternalProfileDataResponse {
        signature_public_key: signature_public_key_to_string(external_profile_data.node_signature_public_key),
        encryption_public_key: encryption_public_key_to_string(external_profile_data.node_encryption_public_key),
    }))
}

async fn get_last_messages_from_inbox_with_branches_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIGetLastMessagesFromInboxWithBranches {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn handle_file_upload(
    node_commands_sender: Sender<NodeCommand>,
    public_key: String,
    encrypted_nonce: String,
    form: warp::multipart::FormData,
) -> Result<Box<dyn warp::Reply + Send>, warp::Rejection> {
    let mut stream = Box::pin(form.filter_map(|part_result| async move {
        if let Ok(part) = part_result {
            shinkai_log(
                ShinkaiLogOption::Identity,
                ShinkaiLogLevel::Debug,
                format!("Received file: {:?}", part).as_str(),
            );
            if let Some(filename) = part.filename() {
                let filename = filename.to_string();
                let stream = part
                    .stream()
                    .map(|res| res.map(|mut buf| buf.copy_to_bytes(buf.remaining()).to_vec()));
                return Some((filename, stream));
            }
        }
        None
    }));

    if let Some((filename, mut file_stream)) = stream.next().await {
        let mut file_data = Vec::new();
        while let Some(Ok(node)) = file_stream.next().await {
            file_data.extend(node);
        }

        let (res_sender, res_receiver) = async_channel::bounded(1);
        node_commands_sender
            .clone()
            .send(NodeCommand::APIAddFileToInboxWithSymmetricKey {
                filename,
                file: file_data,
                public_key,
                encrypted_nonce,
                res: res_sender,
            })
            .map_err(|_| warp::reject::reject())
            .await?;
        let result = res_receiver.recv().await.map_err(|_| warp::reject::reject())?;

        match result {
            Ok(message) => Ok(Box::new(warp::reply::with_status(
                warp::reply::json(&message),
                StatusCode::OK,
            ))),
            Err(error) => Ok(Box::new(warp::reply::with_status(
                warp::reply::json(&error),
                StatusCode::from_u16(error.code).unwrap(),
            ))),
        }
    } else {
        Err(warp::reject::reject())
    }
}

async fn get_public_key_handler(
    node_commands_sender: Sender<NodeCommand>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let node_commands_sender = node_commands_sender.clone();
    let (res_sender, res_receiver) = async_channel::bounded(1);
    node_commands_sender
        .send(NodeCommand::GetPublicKeys(res_sender))
        .await
        .map_err(|_| warp::reject::reject())?; // Send the command to Node
    let (signature_public_key, encryption_public_key) =
        res_receiver.recv().await.map_err(|_| warp::reject::reject())?;
    let signature_public_key_string = signature_public_key_to_string(signature_public_key.clone());
    let encryption_public_key_string = encryption_public_key_to_string(encryption_public_key.clone());
    Ok(warp::reply::json(&GetPublicKeysResponse {
        signature_public_key: signature_public_key_string,
        encryption_public_key: encryption_public_key_string,
    }))
}

async fn get_last_messages_from_inbox_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIGetLastMessagesFromInbox {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn get_last_unread_messages_from_inbox_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIGetLastUnreadMessagesFromInbox {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn get_all_inboxes_for_profile_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIGetAllInboxesForProfile {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn get_all_smart_inboxes_for_profile_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIGetAllSmartInboxesForProfile {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn update_smart_inbox_name_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIUpdateSmartInboxName {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn update_job_to_finished_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIUpdateJobToFinished {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn create_job_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APICreateJob {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn add_agent_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIAddAgent {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn available_agents_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIAvailableAgents {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn job_message_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    let node_commands_sender = node_commands_sender.clone();
    let (res_job_msg_sender, res_job_msg_receiver): (
        async_channel::Sender<Result<SendResponseBodyData, APIError>>,
        async_channel::Receiver<Result<SendResponseBodyData, APIError>>,
    ) = async_channel::bounded(1);
    node_commands_sender
        .send(NodeCommand::APIJobMessage {
            msg: message,
            res: res_job_msg_sender,
        })
        .await
        .map_err(|e| warp::reject::custom(APIError::from(e)))?;
    let job_result = res_job_msg_receiver
        .recv()
        .await
        .map_err(|e| warp::reject::custom(APIError::from(format!("{}", e))))?;
    match job_result {
        Ok(data) => {
            let response_body = SendResponseBody {
                status: "Success".to_string(),
                message: "Job message processed successfully".to_string(),
                data: Some(data),
            };
            Ok(warp::reply::json(&response_body))
        }
        Err(api_error) => Err(warp::reject::custom(api_error)),
    }
}

async fn get_filenames_message_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIGetFilenamesInInbox {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn mark_as_read_up_to_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APIMarkAsReadUpTo {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn create_files_inbox_with_symmetric_key_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    handle_node_command(node_commands_sender, message, |_, message, res_sender| {
        NodeCommand::APICreateFilesInboxWithSymmetricKey {
            msg: message,
            res: res_sender,
        }
    })
    .await
}

async fn create_registration_code_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    let node_commands_sender = node_commands_sender.clone();
    let (res_sender, res_receiver) = async_channel::bounded(1);
    node_commands_sender
        .send(NodeCommand::APICreateRegistrationCode {
            msg: message,
            res: res_sender,
        })
        .await
        .map_err(|_| warp::reject::reject())?; // Send the command to Node
    let result = res_receiver.recv().await.map_err(|_| warp::reject::reject())?;

    match result {
        Ok(code) => {
            let response = serde_json::json!({ "code": code });
            Ok(warp::reply::with_status(warp::reply::json(&response), StatusCode::OK))
        }
        Err(error) => Ok(warp::reply::with_status(
            warp::reply::json(&error),
            StatusCode::from_u16(error.code).unwrap(),
        )),
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct APIUseRegistrationCodeSuccessResponse {
    pub message: String,
    pub node_name: String,
    pub encryption_public_key: String,
    pub identity_public_key: String,
}

async fn use_registration_code_handler(
    node_commands_sender: Sender<NodeCommand>,
    message: ShinkaiMessage,
) -> Result<impl warp::Reply, warp::Rejection> {
    let node_commands_sender = node_commands_sender.clone();
    let (res_sender, res_receiver) = async_channel::bounded(1);
    node_commands_sender
        .send(NodeCommand::APIUseRegistrationCode {
            msg: message,
            res: res_sender,
        })
        .await
        .map_err(|_| warp::reject::reject())?; // Send the command to Node
    let result = res_receiver.recv().await.map_err(|_| warp::reject::reject())?;

    match result {
        Ok(success_response) => {
            let data = serde_json::json!({
                "message": success_response.message,
                "node_name": success_response.node_name,
                "encryption_public_key": success_response.encryption_public_key,
                "identity_public_key": success_response.identity_public_key
            });
            let response = serde_json::json!({
                "status": "success",
                "data": data,
                // TODO: remove the below repeated data  once the Apps have updated
                "message": success_response.message,
                "node_name": success_response.node_name,
                "encryption_public_key": success_response.encryption_public_key,
                "identity_public_key": success_response.identity_public_key
            });
            Ok(warp::reply::with_status(warp::reply::json(&response), StatusCode::OK))
        }
        Err(error) => Ok(warp::reply::with_status(
            warp::reply::json(&error),
            StatusCode::from_u16(error.code).unwrap(),
        )),
    }
}

async fn shinkai_health_handler(
    node_commands_sender: Sender<NodeCommand>,
    node_name: String,
) -> Result<impl warp::Reply, warp::Rejection> {
    let version = env!("CARGO_PKG_VERSION");

    // Create a channel to receive the result
    let (res_sender, res_receiver) = async_channel::bounded(1);

    // Send the command to the node
    node_commands_sender
        .send(NodeCommand::APIIsPristine { res: res_sender })
        .await
        .map_err(|_| warp::reject::reject())?;

    // Receive the result
    let pristine_state = res_receiver.recv().await.map_err(|_| warp::reject::reject())?;

    // Check if there was an error
    if let Err(error) = pristine_state {
        return Ok(warp::reply::json(&json!({ "status": "error", "error": error })));
    }

    // If there was no error, proceed as usual
    Ok(warp::reply::json(
        &json!({ "status": "ok", "version": version, "node_name": node_name, "is_pristine": pristine_state.unwrap() }),
    ))
}

async fn get_all_subidentities_handler(
    node_commands_sender: Sender<NodeCommand>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let node_commands_sender = node_commands_sender.clone();
    let (res_sender, res_receiver) = async_channel::bounded(1);

    node_commands_sender
        .send(NodeCommand::APIGetAllSubidentities { res: res_sender })
        .await
        .map_err(|_| warp::reject::reject())?;

    match res_receiver.recv().await {
        Ok(subidentities) => Ok(warp::reply::json(&subidentities)),
        Err(_) => Err(warp::reject::reject()),
    }
}

async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, warp::Rejection> {
    if let Some(api_error) = err.find::<APIError>() {
        let json = warp::reply::json(api_error);
        return Ok(warp::reply::with_status(
            json,
            StatusCode::from_u16(api_error.code).unwrap(),
        ));
    } else if err.is_not_found() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::NOT_FOUND,
            "Not Found",
            "Please check your URL.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::NOT_FOUND))
    } else if let Some(_) = err.find::<warp::filters::body::BodyDeserializeError>() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::BAD_REQUEST,
            "Invalid Body",
            "Please check your JSON body.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::BAD_REQUEST))
    } else if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "Method Not Allowed",
            "Please check your request method.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::METHOD_NOT_ALLOWED))
    } else if let Some(_) = err.find::<warp::reject::PayloadTooLarge>() {
        let json = warp::reply::json(&APIError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Payload Too Large",
            "The request payload is too large.",
        ));
        Ok(warp::reply::with_status(json, StatusCode::PAYLOAD_TOO_LARGE))
    } else if let Some(_) = err.find::<warp::reject::InvalidQuery>() {
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
