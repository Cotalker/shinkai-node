use ed25519_dalek::Signer;
use shinkai_message_primitives::schemas::shinkai_network::NetworkMessageType;
use shinkai_tcp_relayer::server::{handle_client, Clients, NetworkMessage};
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::task;
use tokio::time::sleep;
use tokio::time::Duration;

// #[tokio::test]
async fn test_handle_client_localhost() {
    // Setup a TCP listener
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Create a shared clients map
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));

    // Spawn a task to accept connections
    let clients_clone = clients.clone();
    let handle = task::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        handle_client(socket, clients_clone).await;
    });

    // Connect to the listener
    let mut socket = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Send a mock identity message
    let identity = "localhost.shinkai";
    let identity_msg = NetworkMessage {
        identity: identity.to_string(),
        message_type: NetworkMessageType::ShinkaiMessage,
        payload: b"Hello, world!".to_vec(),
    };
    send_network_message(&mut socket, &identity_msg).await;
    eprintln!("Sent identity message");
    sleep(Duration::from_millis(100)).await;

    // Check if the client was added to the clients map
    {
        let clients = clients.lock().await;
        assert!(clients.contains_key(identity));
    }

    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_handle_client() {
    // Setup a TCP listener
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Create a shared clients map
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));

    // Spawn a task to accept connections
    let clients_clone = clients.clone();
    let handle = task::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        handle_client(socket, clients_clone).await;
    });

    // Connect to the listener
    let mut socket = tokio::net::TcpStream::connect(addr).await.unwrap();

    // Send a mock identity message
    let identity = "nico_requester.sepolia-shinkai";
    let identity_msg = NetworkMessage {
        identity: identity.to_string(),
        message_type: NetworkMessageType::ShinkaiMessage,
        payload: b"Hello, world!".to_vec(),
    };
    eprintln!("Sending identity message");
    send_network_message(&mut socket, &identity_msg).await;
    eprintln!("Sent identity message");

    // Handle validation
    let mut len_buffer = [0u8; 4];
    eprintln!("Waiting to read validation data length from server");
    socket.read_exact(&mut len_buffer).await.unwrap();
    let validation_data_len = u32::from_be_bytes(len_buffer) as usize;
    eprintln!("Received validation data length: {}", validation_data_len);

    let mut buffer = vec![0u8; validation_data_len];
    eprintln!("Waiting to read validation data from server");
    match socket.read_exact(&mut buffer).await {
        Ok(_) => {
            eprintln!("Read validation data from server");
            let validation_data = String::from_utf8(buffer).unwrap().trim().to_string();
            eprintln!("Received validation data: {}", validation_data);

            // Sign the validation data
            let secret_key_bytes =
                hex::decode("df3f619804a92fdb4057192dc43dd748ea778adc52bc498ce80524c014b81119").unwrap();
            let secret_key_array: [u8; 32] = secret_key_bytes.try_into().expect("slice with incorrect length");
            let secret_key = ed25519_dalek::SigningKey::from_bytes(&secret_key_array);
            let signature = secret_key.sign(validation_data.as_bytes());
            let signature_hex = hex::encode(signature.to_bytes());
            eprintln!("Signed validation data: {}", signature_hex);

            // Send the length of the validation data back to the server
            let signature_len = signature_hex.len() as u32;
            let signature_len_bytes = signature_len.to_be_bytes();

            // Send the length of the signed validation data back to the server
            socket.write_all(&signature_len_bytes).await.unwrap();

            // Send the signed validation data back to the server
            match socket.write_all(signature_hex.as_bytes()).await {
                Ok(_) => eprintln!("Sent signed validation data back to server"),
                Err(e) => eprintln!("Failed to send signed validation data: {}", e),
            }

            // Wait for the server to validate the signature
            // TODO: add code to wait up to X            
        }
        Err(e) => eprintln!("Failed to read validation data: {}", e),
    }

    sleep(Duration::from_millis(1000)).await;

    // Check if the client was added to the clients map
    {
        let clients = clients.lock().await;
        assert!(clients.contains_key(identity));
    }

    // Clean up
    handle.abort();
    eprintln!("Test completed and handle aborted");
}

async fn send_network_message(socket: &mut tokio::net::TcpStream, msg: &NetworkMessage) {
    let encoded_msg = msg.payload.clone();
    let identity = &msg.identity;
    let identity_bytes = identity.as_bytes();
    let identity_length = (identity_bytes.len() as u32).to_be_bytes();

    // Prepare the message with a length prefix and identity length
    let total_length = (encoded_msg.len() as u32 + 1 + identity_bytes.len() as u32 + 4).to_be_bytes(); // Convert the total length to bytes, adding 1 for the header and 4 for the identity length

    let mut data_to_send = Vec::new();
    let header_data_to_send = vec![0x01]; // Message type identifier for ShinkaiMessage
    data_to_send.extend_from_slice(&total_length);
    data_to_send.extend_from_slice(&identity_length);
    data_to_send.extend(identity_bytes);
    data_to_send.extend(header_data_to_send);
    data_to_send.extend_from_slice(&encoded_msg);

    // Print the name and length of each component
    println!("Total length: {}", u32::from_be_bytes(total_length));
    println!("Identity length: {}", u32::from_be_bytes(identity_length));
    println!("Identity bytes length: {}", identity_bytes.len());
    println!("Message type length: 1");
    println!("Payload length: {}", encoded_msg.len());

    socket.write_all(&data_to_send).await.unwrap();
    socket.flush().await.unwrap();
}
