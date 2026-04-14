use futures::{SinkExt, StreamExt};
use invisible_backend::{IncomingMessage, OutgoingMessage, app};
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

    // Connect user "alice"
    let alice_url = format!("ws://{}/ws?user_id=alice", addr);
    let (mut alice_ws, _) = connect_async(&alice_url).await.unwrap();

    // Connect user "bob"
    let bob_url = format!("ws://{}/ws?user_id=bob", addr);
    let (mut bob_ws, _) = connect_async(&bob_url).await.unwrap();

    // Alice sends a message to Bob
    let incoming = IncomingMessage {
        to: "bob".to_string(),
        content: "Hello, Bob!".to_string(),
    };
    let payload = serde_json::to_string(&incoming).unwrap();
    alice_ws
        .send(TgMessage::Text(payload.into()))
        .await
        .unwrap();

    // Bob should receive the message from Alice
    let msg = bob_ws.next().await.unwrap().unwrap();
    if let TgMessage::Text(text) = msg {
        let outgoing: OutgoingMessage = serde_json::from_str(&text).unwrap();
        assert_eq!(outgoing.from, "alice");
        assert_eq!(outgoing.content, "Hello, Bob!");
    } else {
        panic!("Expected text message");
    }
}
