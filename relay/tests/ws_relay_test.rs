use futures::{SinkExt, StreamExt};
use relay::app;
use shared::models::{IncomingMessage, OutgoingMessage};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as TgMessage};

/// Helper function to start the server in the background and return its address
async fn start_test_server() -> SocketAddr {
    let app = app();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    addr
}

#[tokio::test]
async fn given_two_users_when_one_sends_message_then_other_receives_it() {
    let addr = start_test_server().await;

    let alice_url = format!("ws://{}/ws?user_id=alice", addr);
    let (mut alice_ws, _) = connect_async(&alice_url).await.unwrap();

    let bob_url = format!("ws://{}/ws?user_id=bob", addr);
    let (mut bob_ws, _) = connect_async(&bob_url).await.unwrap();

    // 1. Text Message
    let text_incoming = IncomingMessage::Text {
        to: "bob".to_string(),
        id: "msg-123".to_string(),
        content: "Hello, Bob!".to_string(),
    };
    alice_ws
        .send(TgMessage::Text(
            serde_json::to_string(&text_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    // Alice should immediately get a delivery receipt
    let msg = alice_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::DeliveryReceipt { to, message_id } => {
                assert_eq!(to, "bob");
                assert_eq!(message_id, "msg-123");
            }
            _ => panic!("Expected DeliveryReceipt message for Alice"),
        }
    } else {
        panic!("Expected text message");
    }

    // Bob should get the text message
    let msg = bob_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::Text { from, id, content } => {
                assert_eq!(from, "alice");
                assert_eq!(id, "msg-123");
                assert_eq!(content, "Hello, Bob!");
            }
            _ => panic!("Expected Text message"),
        }
    } else {
        panic!("Expected text message");
    }

    // 2. Typing Indicator
    let typing_incoming = IncomingMessage::Typing {
        to: "bob".to_string(),
    };
    alice_ws
        .send(TgMessage::Text(
            serde_json::to_string(&typing_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    let msg = bob_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::Typing { from } => {
                assert_eq!(from, "alice");
            }
            _ => panic!("Expected Typing message"),
        }
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn given_offline_user_when_message_sent_then_received_on_connect() {
    let addr = start_test_server().await;

    // Connect user "charlie"
    let charlie_url = format!("ws://{}/ws?user_id=charlie", addr);
    let (mut charlie_ws, _) = connect_async(&charlie_url).await.unwrap();

    // Charlie sends a message to "dave" (who is offline)
    let incoming = IncomingMessage::Text {
        to: "dave".to_string(),
        id: "msg-offline-1".to_string(),
        content: "Hey Dave, this is waiting for you!".to_string(),
    };
    charlie_ws
        .send(TgMessage::Text(
            serde_json::to_string(&incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    // Give the server a tiny bit of time to queue it
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Now Dave connects
    let dave_url = format!("ws://{}/ws?user_id=dave", addr);
    let (mut dave_ws, _) = connect_async(&dave_url).await.unwrap();

    // Dave should immediately receive the queued message
    let msg = dave_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::Text { from, id, content } => {
                assert_eq!(from, "charlie");
                assert_eq!(id, "msg-offline-1");
                assert_eq!(content, "Hey Dave, this is waiting for you!");
            }
            _ => panic!("Expected queued Text message"),
        }
    } else {
        panic!("Expected text message");
    }

    // Charlie should now receive the delivery receipt since Dave got it
    let msg = charlie_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::DeliveryReceipt { to, message_id } => {
                assert_eq!(to, "dave");
                assert_eq!(message_id, "msg-offline-1");
            }
            _ => panic!("Expected DeliveryReceipt message for Charlie"),
        }
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn given_two_users_when_file_sent_then_received() {
    let addr = start_test_server().await;

    let alice_url = format!("ws://{}/ws?user_id=alice", addr);
    let (mut alice_ws, _) = connect_async(&alice_url).await.unwrap();

    let bob_url = format!("ws://{}/ws?user_id=bob", addr);
    let (mut bob_ws, _) = connect_async(&bob_url).await.unwrap();

    // Alice sends a File message to Bob
    let file_incoming = IncomingMessage::File {
        to: "bob".to_string(),
        id: "msg-file-1".to_string(),
        file_name: "hello.txt".to_string(),
        mime_type: "text/plain".to_string(),
        file_url: "/download/12345".to_string(),
    };
    alice_ws
        .send(TgMessage::Text(
            serde_json::to_string(&file_incoming).unwrap().into(),
        ))
        .await
        .unwrap();

    // Alice should get a delivery receipt
    let msg = alice_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::DeliveryReceipt { to, message_id } => {
                assert_eq!(to, "bob");
                assert_eq!(message_id, "msg-file-1");
            }
            _ => panic!("Expected DeliveryReceipt message for Alice's file"),
        }
    } else {
        panic!("Expected text message");
    }

    // Bob should get the file message
    let msg = bob_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        match outgoing {
            OutgoingMessage::File {
                from,
                id,
                file_name,
                mime_type,
                file_url,
            } => {
                assert_eq!(from, "alice");
                assert_eq!(id, "msg-file-1");
                assert_eq!(file_name, "hello.txt");
                assert_eq!(mime_type, "text/plain");
                assert_eq!(file_url, "/download/12345");
            }
            _ => panic!("Expected File message"),
        }
    } else {
        panic!("Expected text message");
    }
}
