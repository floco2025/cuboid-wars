use crate::ChatClient;
use anyhow::Result;
#[allow(clippy::wildcard_imports)]
use common::{io::MessageStream, protocol::*};
use quinn::{Connection, ConnectionError};
use std::sync::Arc;
use tokio::sync::Mutex;

// ============================================================================
// Message Handling
// ============================================================================

pub async fn send_message(connection: &Connection, msg: &ClientMessage) -> Result<()> {
    let stream = MessageStream::new(connection);
    stream.send(msg).await
}

pub async fn receive_messages(connection: Arc<Connection>, client: Arc<Mutex<ChatClient>>) {
    let stream = MessageStream::new(&connection);

    loop {
        match stream.recv::<ServerMessage>().await {
            Ok(msg) => {
                let mut client_guard = client.lock().await;
                client_guard.process_message(msg);
            }
            Err(e) => {
                if let Some(conn_err) = e.downcast_ref::<ConnectionError>() {
                    match conn_err {
                        ConnectionError::ApplicationClosed { .. } => {
                            println!("Server closed the connection");
                        }
                        ConnectionError::TimedOut => {
                            eprintln!("Server connection timed out");
                        }
                        ConnectionError::LocallyClosed => {
                            eprintln!("Connection to server closed locally");
                        }
                        _ => {
                            eprintln!("Connection error: {e}");
                        }
                    }
                } else {
                    eprintln!("Error receiving message: {e}");
                }
                return;
            }
        }
    }
}
