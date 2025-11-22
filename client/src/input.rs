use crate::ChatClient;
use anyhow::Result;
#[allow(clippy::wildcard_imports)]
use common::{io::MessageStream, protocol::*};
use quinn::Connection;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::sync::Mutex;

// ============================================================================
// User Input
// ============================================================================

pub async fn user_input(connection: Arc<Connection>, client: Arc<Mutex<ChatClient>>) {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);

    loop {
        let mut text = String::new();
        match reader.read_line(&mut text).await {
            Ok(n) => {
                // Make EOF (Ctrl-D) behave the same as /quit
                if n == 0 {
                    text = "/quit".to_string();
                }

                text = text.trim().to_string();
                if text.is_empty() {
                    continue;
                }

                if text.starts_with('/') {
                    if let Err(e) = command(&text, &connection, &client).await {
                        eprintln!("Error executing command: {e}");
                    }
                } else if let Err(e) = send_message(&connection, &ClientMessage::Say(CSay { text })).await {
                    eprintln!("Error sending message: {e}");
                    break;
                }
            }
            Err(e) => {
                eprintln!("Error reading input: {e}");
                break;
            }
        }
    }
}

async fn command(command: &str, connection: &Connection, client: &Arc<Mutex<ChatClient>>) -> Result<()> {
    let parts: Vec<&str> = command[1..].splitn(2, ' ').collect();

    match parts.first() {
        Some(&"name") => {
            if let Some(name) = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                send_message(connection, &ClientMessage::Name(CName { name: name.to_string() })).await?;
            } else {
                println!("Usage: /name <new name>");
            }
        }
        Some(&"login") => {
            if let Some(name) = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                send_message(connection, &ClientMessage::Login(CLogin { name: name.to_string() })).await?;
            } else {
                println!("Usage: /login <name>");
            }
        }
        Some(&"who") => {
            let names = client.lock().await.get_all_names();
            if names.is_empty() {
                println!("No participants.");
            } else {
                println!("Participants:");
                for name in names {
                    println!("  {name}");
                }
            }
        }
        Some(&"remove") => {
            if let Some(name) = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                let client_guard = client.lock().await;
                match client_guard.get_id_by_name(name) {
                    Some(id) => {
                        drop(client_guard);
                        send_message(connection, &ClientMessage::Remove(CRemove { id })).await?;
                    }
                    None => {
                        println!("Participant '{name}' not found");
                    }
                }
            } else {
                println!("Usage: /remove <name>");
            }
        }
        Some(&"quit") => {
            println!("Goodbye!");
            send_message(connection, &ClientMessage::Logoff(CLogoff {})).await?;
            // Don't exit - wait for server to gracefully close the connection. The receive_messages
            // task will detect the closure and exit.
        }
        Some(&"help" | &"?") => {
            println!("Available commands:");
            println!("  /name <new name>  - Change your name");
            println!("  /login <name>     - Login with a name (for testing)");
            println!("  /who              - List current participants");
            println!("  /remove <name>    - Remove a participant from the chat");
            println!("  /quit             - Exit the chat");
            println!("  /help or /?       - Show this help message");
        }
        Some(cmd) => {
            println!("Unknown command: /{cmd}");
            println!("Type /help or /? for available commands");
        }
        None => {
            println!("Empty command");
            println!("Type /help or /? for available commands");
        }
    }

    Ok(())
}

async fn send_message(connection: &Connection, msg: &ClientMessage) -> Result<()> {
    let stream = MessageStream::new(connection);
    stream.send(msg).await
}
