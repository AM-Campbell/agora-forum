use crate::api::ApiClient;
use crate::identity::Identity;

pub async fn inbox(api: &ApiClient) -> Result<(), String> {
    let resp = api.get_inbox().await?;

    if resp.conversations.is_empty() {
        println!("No conversations yet.");
        return Ok(());
    }

    println!("{:<16} {:<8} {}", "User", "Messages", "Last message");
    println!("{}", "-".repeat(48));

    for conv in &resp.conversations {
        println!(
            "{:<16} {:<8} {}",
            conv.username, conv.message_count, conv.last_message_at
        );
    }

    Ok(())
}

pub async fn send(
    api: &ApiClient,
    identity: &Identity,
    username: &str,
    file: Option<&str>,
) -> Result<(), String> {
    // Get recipient's public key
    let key_resp = api.get_user_public_key(username).await?;

    // Get message body
    let body = match file {
        Some("-") => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("Failed to read stdin: {}", e))?;
            buf
        }
        Some(path) => {
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?
        }
        None => {
            // Open editor
            let content = "# Write your direct message below this line\n\n";
            match crate::editor::open_editor(&format!("dm_{}", username), content) {
                Ok(Some(body)) => body,
                Ok(None) => {
                    println!("Empty message, aborting.");
                    return Ok(());
                }
                Err(e) => return Err(format!("Editor error: {}", e)),
            }
        }
    };

    let body = body.trim();
    if body.is_empty() {
        println!("Empty message, aborting.");
        return Ok(());
    }

    // Encrypt
    let (ciphertext, nonce) = identity.encrypt_for(&key_resp.public_key, body)?;

    // Send
    let resp = api.send_dm(username, &ciphertext, &nonce).await?;
    println!("Message sent to {} (id: {})", username, resp.dm_id);

    Ok(())
}

pub async fn read_conversation(
    api: &ApiClient,
    identity: &Identity,
    username: &str,
    page: i64,
) -> Result<(), String> {
    let resp = api.get_conversation(username, page).await?;

    if resp.messages.is_empty() {
        println!("No messages with {}.", username);
        return Ok(());
    }

    println!("Conversation with {}\n", username);

    for msg in &resp.messages {
        // Determine sender's public key for decryption
        let sender_pub = if msg.sender == username {
            &resp.partner_public_key
        } else {
            // Message from us — we need our own public key for the shared secret
            // But actually crypto_box is symmetric: same shared key regardless of direction
            &resp.partner_public_key
        };

        match identity.decrypt_from(sender_pub, &msg.ciphertext, &msg.nonce) {
            Ok(plaintext) => {
                println!("[{}] {}:", msg.created_at, msg.sender);
                println!("  {}", plaintext);
                println!();
            }
            Err(e) => {
                println!("[{}] {}: <decryption failed: {}>", msg.created_at, msg.sender, e);
                println!();
            }
        }
    }

    println!(
        "Page {}/{} — {} message(s)",
        resp.page, resp.total_pages, resp.messages.len()
    );

    Ok(())
}
