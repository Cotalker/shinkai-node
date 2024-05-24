use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use async_channel::{bounded, Receiver, Sender};
use serde_json::Value;
use shinkai_message_primitives::shinkai_utils::{
    encryption::{
        encryption_public_key_to_string, encryption_secret_key_to_string, unsafe_deterministic_encryption_keypair,
    },
    shinkai_logging::init_default_tracing,
    shinkai_message_builder::ShinkaiMessageBuilder,
    signatures::{
        clone_signature_secret_key, signature_public_key_to_string, signature_secret_key_to_string,
        unsafe_deterministic_signature_keypair,
    },
};
use shinkai_node::network::{node::NodeCommand, node_api::APIError, Node};
use shinkai_tcp_relayer::TCPProxy;
use shinkai_vector_resources::utils::hash_string;
use tokio::{net::TcpListener, runtime::Runtime};

use crate::it::utils::{
    node_test_api::api_registration_device_node_profile_main, node_test_local::local_registration_profile_node,
    shinkai_testing_framework::ShinkaiTestingFramework, vecfs_test_utils::fetch_last_messages,
};

use super::utils::db_handlers::setup;

#[test]
fn tcp_proxy_test_identity() {
    std::env::set_var("WELCOME_MESSAGE", "false");
    init_default_tracing();
    setup();
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let node1_identity_name = "@@node1_test_with_proxy.sepolia-shinkai";
        let node2_identity_name = "@@node2_test.sepolia-shinkai";
        let node1_profile_name = "main";
        let node2_profile_name = "main";
        let node1_device_name = "device1";

        let (node1_identity_sk, node1_identity_pk) = unsafe_deterministic_signature_keypair(0);
        let (node1_encryption_sk, node1_encryption_pk) = unsafe_deterministic_encryption_keypair(0);
        let _node1_encryption_sk_clone = node1_encryption_sk.clone();
        let node1_encryption_sk_clone2 = node1_encryption_sk.clone();

        let (node2_identity_sk, node2_identity_pk) = unsafe_deterministic_signature_keypair(1);
        let (node2_encryption_sk, node2_encryption_pk) = unsafe_deterministic_encryption_keypair(1);
        let node2_encryption_sk_clone = node2_encryption_sk.clone();

        let tcp_proxy_identity_name = "@@tcp_tests_proxy.sepolia-shinkai";
        let (tcp_proxy_identity_sk, tcp_proxy_identity_pk) = unsafe_deterministic_signature_keypair(2);
        let (tcp_proxy_encryption_sk, tcp_proxy_encryption_pk) = unsafe_deterministic_encryption_keypair(2);

        eprintln!(
            "TCP Proxy encryption sk: {:?}",
            encryption_secret_key_to_string(tcp_proxy_encryption_sk.clone())
        );
        eprintln!(
            "TCP Proxy encryption pk: {:?}",
            encryption_public_key_to_string(tcp_proxy_encryption_pk)
        );
        eprintln!(
            "TCP Proxy identity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&tcp_proxy_identity_sk))
        );
        eprintln!(
            "TCP Proxy identity pk: {:?}",
            signature_public_key_to_string(tcp_proxy_identity_pk)
        );

        let _node1_identity_sk_clone = clone_signature_secret_key(&node1_identity_sk);
        let _node2_identity_sk_clone = clone_signature_secret_key(&node2_identity_sk);

        let (node1_profile_identity_sk, node1_profile_identity_pk) = unsafe_deterministic_signature_keypair(100);
        let (node1_profile_encryption_sk, node1_profile_encryption_pk) = unsafe_deterministic_encryption_keypair(100);

        let (node2_profile_identity_sk, node2_profile_identity_pk) = unsafe_deterministic_signature_keypair(101);
        let (node2_profile_encryption_sk, node2_profile_encryption_pk) = unsafe_deterministic_encryption_keypair(101);

        let node1_subencryption_sk_clone = node1_profile_encryption_sk.clone();
        let node2_subencryption_sk_clone = node2_profile_encryption_sk.clone();

        let _node1_subidentity_sk_clone = clone_signature_secret_key(&node1_profile_identity_sk);
        let _node2_subidentity_sk_clone = clone_signature_secret_key(&node2_profile_identity_sk);

        let (node1_device_identity_sk, _node1_device_identity_pk) = unsafe_deterministic_signature_keypair(200);
        let (node1_device_encryption_sk, _node1_device_encryption_pk) = unsafe_deterministic_encryption_keypair(200);

        let (node1_commands_sender, node1_commands_receiver): (Sender<NodeCommand>, Receiver<NodeCommand>) =
            bounded(100);
        let (node2_commands_sender, node2_commands_receiver): (Sender<NodeCommand>, Receiver<NodeCommand>) =
            bounded(100);

        let node1_db_path = format!("db_tests/{}", hash_string(node1_identity_name));
        let node1_fs_db_path = format!("db_tests/vector_fs{}", hash_string(node1_identity_name));
        let node2_db_path = format!("db_tests/{}", hash_string(node2_identity_name));
        let node2_fs_db_path = format!("db_tests/vector_fs{}", hash_string(node2_identity_name));

        // Create node1 and node2
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let node1 = Node::new(
            node1_identity_name.to_string(),
            addr1,
            clone_signature_secret_key(&node1_identity_sk),
            node1_encryption_sk,
            0,
            node1_commands_receiver,
            node1_db_path,
            "".to_string(),
            Some(tcp_proxy_identity_name.to_string()),
            true,
            vec![],
            None,
            node1_fs_db_path,
            None,
            None,
        )
        .await;

        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);
        let node2 = Node::new(
            node2_identity_name.to_string(),
            addr2,
            clone_signature_secret_key(&node2_identity_sk),
            node2_encryption_sk,
            0,
            node2_commands_receiver,
            node2_db_path,
            "".to_string(),
            None,
            true,
            vec![],
            None,
            node2_fs_db_path,
            None,
            None,
        )
        .await;

        // Printing
        eprintln!(
            "Node 1 encryption sk: {:?}",
            encryption_secret_key_to_string(node1_encryption_sk_clone2.clone())
        );
        eprintln!(
            "Node 1 encryption pk: {:?}",
            encryption_public_key_to_string(node1_encryption_pk)
        );

        eprintln!(
            "Node 2 encryption sk: {:?}",
            encryption_secret_key_to_string(node2_encryption_sk_clone)
        );
        eprintln!(
            "Node 2 encryption pk: {:?}",
            encryption_public_key_to_string(node2_encryption_pk)
        );

        eprintln!(
            "Node 1 identity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&node1_identity_sk))
        );
        eprintln!(
            "Node 1 identity pk: {:?}",
            signature_public_key_to_string(node1_identity_pk)
        );

        eprintln!(
            "Node 2 identity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&node2_identity_sk))
        );
        eprintln!(
            "Node 2 identity pk: {:?}",
            signature_public_key_to_string(node2_identity_pk)
        );

        eprintln!(
            "Node 1 subidentity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&node1_profile_identity_sk))
        );
        eprintln!(
            "Node 1 subidentity pk: {:?}",
            signature_public_key_to_string(node1_profile_identity_pk)
        );

        eprintln!(
            "Node 2 subidentity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&node2_profile_identity_sk))
        );
        eprintln!(
            "Node 2 subidentity pk: {:?}",
            signature_public_key_to_string(node2_profile_identity_pk)
        );

        eprintln!(
            "Node 1 subencryption sk: {:?}",
            encryption_secret_key_to_string(node1_subencryption_sk_clone.clone())
        );
        eprintln!(
            "Node 1 subencryption pk: {:?}",
            encryption_public_key_to_string(node1_profile_encryption_pk)
        );

        eprintln!(
            "Node 2 subencryption sk: {:?}",
            encryption_secret_key_to_string(node2_subencryption_sk_clone.clone())
        );
        eprintln!(
            "Node 2 subencryption pk: {:?}",
            encryption_public_key_to_string(node2_profile_encryption_pk)
        );

        eprintln!("Starting nodes");
        // Start node1 and node2
        let node1_clone = Arc::clone(&node1);
        let node1_handler = tokio::spawn(async move {
            eprintln!("\n\n");
            eprintln!("Starting node 1");
            let _ = node1_clone.lock().await.start().await;
        });

        let node1_abort_handler = node1_handler.abort_handle();

        let node2_clone = Arc::clone(&node2);
        let node2_handler = tokio::spawn(async move {
            eprintln!("\n\n");
            eprintln!("Starting node 2");
            let _ = node2_clone.lock().await.start().await;
        });
        let node2_abort_handler = node2_handler.abort_handle();

        let interactions_handler = tokio::spawn(async move {
            eprintln!("Starting interactions");
            eprintln!("Registration of Subidentities");

            // start a tcp proxy node
            // start node with proxy node ENV variable set
            // start another node that has some files shared
            // node 1 should be able to access the files on node 2 using the proxy

            // Creates a TCPProxy instance
            let proxy = TCPProxy::new(
                Some(tcp_proxy_identity_sk),
                Some(tcp_proxy_encryption_sk),
                Some(tcp_proxy_identity_name.to_string()),
            )
            .await
            .unwrap();

            // Setup a TCP listener
            // Info from: https://shinkai-contracts.pages.dev/identity/tcp_tests_proxy.sepolia-shinkai
            let listener = TcpListener::bind("127.0.0.1:8084").await.unwrap();

            // Spawn a task to accept connections
            let _tcp_handle = tokio::spawn({
                let proxy = proxy.clone();
                async move {
                    let (socket, _) = listener.accept().await.unwrap();
                    proxy.handle_client(socket).await;
                }
            });

            // Register a Profile in Node1 and verifies it
            {
                eprintln!("Register a Device with main profile in Node1 and verify it");
                api_registration_device_node_profile_main(
                    node1_commands_sender.clone(),
                    node1_profile_name,
                    node1_identity_name,
                    node1_encryption_pk,
                    node1_device_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_device_identity_sk),
                    node1_profile_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node1_device_name,
                )
                .await;
            }

            // Register a Profile in Node2 and verifies it
            {
                eprintln!("Register a Profile in Node2 and verify it");
                local_registration_profile_node(
                    node2_commands_sender.clone(),
                    node2_profile_name,
                    node2_identity_name,
                    node2_subencryption_sk_clone.clone(),
                    node2_encryption_pk,
                    clone_signature_secret_key(&node2_profile_identity_sk),
                    1,
                )
                .await;
            }

            tokio::time::sleep(Duration::from_secs(3)).await;

            // Shinkai Testing Framework
            let node_2_testing_framework = ShinkaiTestingFramework::new(
                node2_commands_sender.clone(),
                node2_profile_identity_sk.clone(),
                node2_profile_encryption_sk.clone(),
                node2_encryption_pk,
                node2_identity_name.to_string(),
                node2_profile_name.to_string(),
            );

            //
            // Creating a folder and uploading some files to the vector db
            //
            eprintln!("\n\n### Creating a folder and uploading some files to the vector db \n\n");
            {
                // Create /shinkai_sharing folder
                node_2_testing_framework.create_folder("/", "shinkai_sharing").await;
                node_2_testing_framework
                    .upload_file("/shinkai_sharing", "files/shinkai_intro.vrkai")
                    .await;
                node_2_testing_framework.make_folder_shareable("/shinkai_sharing").await;

                // For Debugging
                node_2_testing_framework.retrieve_file_info("/", true).await;
                node_2_testing_framework.show_available_shared_items().await;
            }
            {
                eprintln!("\n\n### Sending message from node 1 to TCP Relay to node 1 requesting shared folders*\n");

                let unchanged_message = ShinkaiMessageBuilder::vecfs_available_shared_items(
                    None,
                    node2_identity_name.to_string(),
                    node2_profile_name.to_string(),
                    node1_profile_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node2_encryption_pk,
                    node1_identity_name.to_string().clone(),
                    node1_profile_name.to_string().clone(),
                    node2_identity_name.to_string(),
                    node2_profile_name.to_string().clone(),
                )
                .unwrap();

                // eprintln!("\n\n unchanged message: {:?}", unchanged_message);

                #[allow(clippy::type_complexity)]
                let (res_send_msg_sender, res_send_msg_receiver): (
                    async_channel::Sender<Result<Value, APIError>>,
                    async_channel::Receiver<Result<Value, APIError>>,
                ) = async_channel::bounded(1);

                node1_commands_sender
                    .send(NodeCommand::APIAvailableSharedItems {
                        msg: unchanged_message,
                        res: res_send_msg_sender,
                    })
                    .await
                    .unwrap();

                let send_result = res_send_msg_receiver.recv().await.unwrap();
                eprint!("send_result: {:?}", send_result);
                assert!(send_result.is_ok(), "Failed to get APIAvailableSharedItems");
                tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;

                let node2_last_messages = fetch_last_messages(&node2_commands_sender, 2)
                    .await
                    .expect("Failed to fetch last messages for node 2");

                eprintln!("Node 2 last messages: {:?}", node2_last_messages);
                eprintln!("\n\n");

                let node1_last_messages = fetch_last_messages(&node1_commands_sender, 2)
                    .await
                    .expect("Failed to fetch last messages for node 1");

                eprintln!("\n\nNode 1 last messages: {:?}", node1_last_messages);
                eprintln!("\n\n");
            }
            {
                // Dont forget to do this at the end
                node1_abort_handler.abort();
                node2_abort_handler.abort();
            }
        });

        // Wait for all tasks to complete
        let result = tokio::try_join!(node1_handler, node2_handler, interactions_handler);
        match result {
            Ok(_) => {}
            Err(e) => {
                // Check if the error is because one of the tasks was aborted
                if e.is_cancelled() {
                    eprintln!("One of the tasks was aborted, but this is expected.");
                } else {
                    // If the error is not due to an abort, then it's unexpected
                    panic!("An unexpected error occurred: {:?}", e);
                }
            }
        }
    });

    rt.shutdown_background();
}

#[test]
fn tcp_proxy_test_localhost() {
    std::env::set_var("WELCOME_MESSAGE", "false");
    init_default_tracing();
    setup();
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let node1_identity_name = "@@localhost.sepolia-shinkai";
        let node2_identity_name = "@@node2_test.sepolia-shinkai";
        let node1_profile_name = "main";
        let node2_profile_name = "main";
        let node1_device_name = "device1";

        let (node1_identity_sk, node1_identity_pk) = unsafe_deterministic_signature_keypair(0);
        let (node1_encryption_sk, node1_encryption_pk) = unsafe_deterministic_encryption_keypair(0);
        let _node1_encryption_sk_clone = node1_encryption_sk.clone();
        let node1_encryption_sk_clone2 = node1_encryption_sk.clone();

        let (node2_identity_sk, node2_identity_pk) = unsafe_deterministic_signature_keypair(1);
        let (node2_encryption_sk, node2_encryption_pk) = unsafe_deterministic_encryption_keypair(1);
        let node2_encryption_sk_clone = node2_encryption_sk.clone();

        let tcp_proxy_identity_name = "@@tcp_tests_proxy.sepolia-shinkai";
        let (tcp_proxy_identity_sk, tcp_proxy_identity_pk) = unsafe_deterministic_signature_keypair(2);
        let (tcp_proxy_encryption_sk, tcp_proxy_encryption_pk) = unsafe_deterministic_encryption_keypair(2);

        eprintln!(
            "TCP Proxy encryption sk: {:?}",
            encryption_secret_key_to_string(tcp_proxy_encryption_sk.clone())
        );
        eprintln!(
            "TCP Proxy encryption pk: {:?}",
            encryption_public_key_to_string(tcp_proxy_encryption_pk)
        );
        eprintln!(
            "TCP Proxy identity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&tcp_proxy_identity_sk))
        );
        eprintln!(
            "TCP Proxy identity pk: {:?}",
            signature_public_key_to_string(tcp_proxy_identity_pk)
        );

        let _node1_identity_sk_clone = clone_signature_secret_key(&node1_identity_sk);
        let _node2_identity_sk_clone = clone_signature_secret_key(&node2_identity_sk);

        let (node1_profile_identity_sk, node1_profile_identity_pk) = unsafe_deterministic_signature_keypair(100);
        let (node1_profile_encryption_sk, node1_profile_encryption_pk) = unsafe_deterministic_encryption_keypair(100);

        let (node2_profile_identity_sk, node2_profile_identity_pk) = unsafe_deterministic_signature_keypair(101);
        let (node2_profile_encryption_sk, node2_profile_encryption_pk) = unsafe_deterministic_encryption_keypair(101);

        let node1_subencryption_sk_clone = node1_profile_encryption_sk.clone();
        let node2_subencryption_sk_clone = node2_profile_encryption_sk.clone();

        let _node1_subidentity_sk_clone = clone_signature_secret_key(&node1_profile_identity_sk);
        let _node2_subidentity_sk_clone = clone_signature_secret_key(&node2_profile_identity_sk);

        let (node1_device_identity_sk, _node1_device_identity_pk) = unsafe_deterministic_signature_keypair(200);
        let (node1_device_encryption_sk, _node1_device_encryption_pk) = unsafe_deterministic_encryption_keypair(200);

        let (node1_commands_sender, node1_commands_receiver): (Sender<NodeCommand>, Receiver<NodeCommand>) =
            bounded(100);
        let (node2_commands_sender, node2_commands_receiver): (Sender<NodeCommand>, Receiver<NodeCommand>) =
            bounded(100);

        let node1_db_path = format!("db_tests/{}", hash_string(node1_identity_name));
        let node1_fs_db_path = format!("db_tests/vector_fs{}", hash_string(node1_identity_name));
        let node2_db_path = format!("db_tests/{}", hash_string(node2_identity_name));
        let node2_fs_db_path = format!("db_tests/vector_fs{}", hash_string(node2_identity_name));

        // Create node1 and node2
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let node1 = Node::new(
            node1_identity_name.to_string(),
            addr1,
            clone_signature_secret_key(&node1_identity_sk),
            node1_encryption_sk,
            0,
            node1_commands_receiver,
            node1_db_path,
            "".to_string(),
            Some(tcp_proxy_identity_name.to_string()),
            true,
            vec![],
            None,
            node1_fs_db_path,
            None,
            None,
        )
        .await;

        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);
        let node2 = Node::new(
            node2_identity_name.to_string(),
            addr2,
            clone_signature_secret_key(&node2_identity_sk),
            node2_encryption_sk,
            0,
            node2_commands_receiver,
            node2_db_path,
            "".to_string(),
            None,
            true,
            vec![],
            None,
            node2_fs_db_path,
            None,
            None,
        )
        .await;

        // Printing
        eprintln!(
            "Node 1 encryption sk: {:?}",
            encryption_secret_key_to_string(node1_encryption_sk_clone2.clone())
        );
        eprintln!(
            "Node 1 encryption pk: {:?}",
            encryption_public_key_to_string(node1_encryption_pk)
        );

        eprintln!(
            "Node 2 encryption sk: {:?}",
            encryption_secret_key_to_string(node2_encryption_sk_clone)
        );
        eprintln!(
            "Node 2 encryption pk: {:?}",
            encryption_public_key_to_string(node2_encryption_pk)
        );

        eprintln!(
            "Node 1 identity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&node1_identity_sk))
        );
        eprintln!(
            "Node 1 identity pk: {:?}",
            signature_public_key_to_string(node1_identity_pk)
        );

        eprintln!(
            "Node 2 identity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&node2_identity_sk))
        );
        eprintln!(
            "Node 2 identity pk: {:?}",
            signature_public_key_to_string(node2_identity_pk)
        );

        eprintln!(
            "Node 1 subidentity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&node1_profile_identity_sk))
        );
        eprintln!(
            "Node 1 subidentity pk: {:?}",
            signature_public_key_to_string(node1_profile_identity_pk)
        );

        eprintln!(
            "Node 2 subidentity sk: {:?}",
            signature_secret_key_to_string(clone_signature_secret_key(&node2_profile_identity_sk))
        );
        eprintln!(
            "Node 2 subidentity pk: {:?}",
            signature_public_key_to_string(node2_profile_identity_pk)
        );

        eprintln!(
            "Node 1 subencryption sk: {:?}",
            encryption_secret_key_to_string(node1_subencryption_sk_clone.clone())
        );
        eprintln!(
            "Node 1 subencryption pk: {:?}",
            encryption_public_key_to_string(node1_profile_encryption_pk)
        );

        eprintln!(
            "Node 2 subencryption sk: {:?}",
            encryption_secret_key_to_string(node2_subencryption_sk_clone.clone())
        );
        eprintln!(
            "Node 2 subencryption pk: {:?}",
            encryption_public_key_to_string(node2_profile_encryption_pk)
        );

        eprintln!("Starting nodes");
        // Start node1 and node2
        let node1_clone = Arc::clone(&node1);
        let node1_handler = tokio::spawn(async move {
            eprintln!("\n\n");
            eprintln!("Starting node 1");
            let _ = node1_clone.lock().await.start().await;
        });

        let node1_abort_handler = node1_handler.abort_handle();

        let node2_clone = Arc::clone(&node2);
        let node2_handler = tokio::spawn(async move {
            eprintln!("\n\n");
            eprintln!("Starting node 2");
            let _ = node2_clone.lock().await.start().await;
        });
        let node2_abort_handler = node2_handler.abort_handle();

        let interactions_handler = tokio::spawn(async move {
            eprintln!("Starting interactions");
            eprintln!("Registration of Subidentities");

            // start a tcp proxy node
            // start node with proxy node ENV variable set
            // start another node that has some files shared
            // node 1 should be able to access the files on node 2 using the proxy

            // Creates a TCPProxy instance
            let proxy = TCPProxy::new(
                Some(tcp_proxy_identity_sk),
                Some(tcp_proxy_encryption_sk),
                Some(tcp_proxy_identity_name.to_string()),
            )
            .await
            .unwrap();

            // Setup a TCP listener
            // Info from: https://shinkai-contracts.pages.dev/identity/tcp_tests_proxy.sepolia-shinkai
            let listener = TcpListener::bind("127.0.0.1:8084").await.unwrap();

            // Spawn a task to accept connections
            let _tcp_handle = tokio::spawn({
                let proxy = proxy.clone();
                async move {
                    let (socket, _) = listener.accept().await.unwrap();
                    proxy.handle_client(socket).await;
                }
            });

            // Register a Profile in Node1 and verifies it
            {
                eprintln!("Register a Device with main profile in Node1 and verify it");
                api_registration_device_node_profile_main(
                    node1_commands_sender.clone(),
                    node1_profile_name,
                    node1_identity_name,
                    node1_encryption_pk,
                    node1_device_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_device_identity_sk),
                    node1_profile_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node1_device_name,
                )
                .await;
            }

            // Register a Profile in Node2 and verifies it
            {
                eprintln!("Register a Profile in Node2 and verify it");
                local_registration_profile_node(
                    node2_commands_sender.clone(),
                    node2_profile_name,
                    node2_identity_name,
                    node2_subencryption_sk_clone.clone(),
                    node2_encryption_pk,
                    clone_signature_secret_key(&node2_profile_identity_sk),
                    1,
                )
                .await;
            }

            tokio::time::sleep(Duration::from_secs(3)).await;

            let node_2_testing_framework = ShinkaiTestingFramework::new(
                node2_commands_sender.clone(),
                node2_profile_identity_sk.clone(),
                node2_profile_encryption_sk.clone(),
                node2_encryption_pk,
                node2_identity_name.to_string(),
                node2_profile_name.to_string(),
            );

            //
            // Creating a folder and uploading some files to the vector db
            //
            eprintln!("\n\n### Creating a folder and uploading some files to the vector db \n\n");
            {
                // Create /shinkai_sharing folder
                node_2_testing_framework.create_folder("/", "shinkai_sharing").await;
                node_2_testing_framework
                    .upload_file("/shinkai_sharing", "files/shinkai_intro.vrkai")
                    .await;
                node_2_testing_framework.make_folder_shareable("/shinkai_sharing").await;

                // For Debugging
                node_2_testing_framework.retrieve_file_info("/", true).await;
                node_2_testing_framework.show_available_shared_items().await;
            }
            {
                eprintln!("\n\n### Sending message from node 1 to TCP Relay to node 1 requesting shared folders*\n");

                let unchanged_message = ShinkaiMessageBuilder::vecfs_available_shared_items(
                    None,
                    node2_identity_name.to_string(),
                    node2_profile_name.to_string(),
                    node1_profile_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node2_encryption_pk,
                    node1_identity_name.to_string().clone(),
                    node1_profile_name.to_string().clone(),
                    node2_identity_name.to_string(),
                    node2_profile_name.to_string().clone(),
                )
                .unwrap();

                // eprintln!("\n\n unchanged message: {:?}", unchanged_message);

                #[allow(clippy::type_complexity)]
                let (res_send_msg_sender, res_send_msg_receiver): (
                    async_channel::Sender<Result<Value, APIError>>,
                    async_channel::Receiver<Result<Value, APIError>>,
                ) = async_channel::bounded(1);

                node1_commands_sender
                    .send(NodeCommand::APIAvailableSharedItems {
                        msg: unchanged_message,
                        res: res_send_msg_sender,
                    })
                    .await
                    .unwrap();

                let send_result = res_send_msg_receiver.recv().await.unwrap();
                eprint!("send_result: {:?}", send_result);
                assert!(send_result.is_ok(), "Failed to get APIAvailableSharedItems");
                tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;

                let node2_last_messages = fetch_last_messages(&node2_commands_sender, 2)
                    .await
                    .expect("Failed to fetch last messages for node 2");

                eprintln!("Node 2 last messages: {:?}", node2_last_messages);
                eprintln!("\n\n");

                let node1_last_messages = fetch_last_messages(&node1_commands_sender, 2)
                    .await
                    .expect("Failed to fetch last messages for node 1");

                eprintln!("\n\nNode 1 last messages: {:?}", node1_last_messages);
                eprintln!("\n\n");
            }
            {
                // Dont forget to do this at the end
                node1_abort_handler.abort();
                node2_abort_handler.abort();
            }
        });

        // Wait for all tasks to complete
        let result = tokio::try_join!(node1_handler, node2_handler, interactions_handler);
        match result {
            Ok(_) => {}
            Err(e) => {
                // Check if the error is because one of the tasks was aborted
                if e.is_cancelled() {
                    eprintln!("One of the tasks was aborted, but this is expected.");
                } else {
                    // If the error is not due to an abort, then it's unexpected
                    panic!("An unexpected error occurred: {:?}", e);
                }
            }
        }
    });

    rt.shutdown_background();
}
