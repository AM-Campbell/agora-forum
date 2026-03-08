use serde::{Deserialize, Serialize};
use std::io::{self, Write};

use crate::config;

#[derive(Serialize, Deserialize)]
struct ProfileExport {
    version: u32,
    global: GlobalSection,
    #[serde(default)]
    servers: Vec<ServerEntry>,
}

#[derive(Serialize, Deserialize)]
struct GlobalSection {
    socks_proxy: String,
    editor: Option<String>,
    reply_context: usize,
    default_server: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ServerEntry {
    address: String,
    username: String,
    server_name: Option<String>,
    identity_key: String,
}

pub fn export(output: Option<&str>) -> Result<(), String> {
    let global = config::GlobalConfig::load_or_default();
    let servers = config::list_servers();

    let mut entries = Vec::new();
    for srv in &servers {
        let key_path = config::server_identity_path(&srv.server);
        let identity_key = std::fs::read_to_string(&key_path)
            .map_err(|e| format!("Failed to read identity key for {}: {}", srv.server, e))?;

        entries.push(ServerEntry {
            address: srv.server.clone(),
            username: srv.username.clone(),
            server_name: srv.server_name.clone(),
            identity_key: identity_key.trim().to_string(),
        });
    }

    let profile = ProfileExport {
        version: 1,
        global: GlobalSection {
            socks_proxy: global.socks_proxy,
            editor: global.editor,
            reply_context: global.reply_context,
            default_server: global.default_server,
        },
        servers: entries,
    };

    let content =
        toml::to_string_pretty(&profile).map_err(|e| format!("Failed to serialize profile: {}", e))?;

    let path = output.unwrap_or("agora-profile.toml");

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .and_then(|mut f| f.write_all(content.as_bytes()))
            .map_err(|e| format!("Failed to write profile: {}", e))?;
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, &content).map_err(|e| format!("Failed to write profile: {}", e))?;
    }

    println!("Profile exported to: {}", path);
    println!("Contains {} server(s).", profile.servers.len());
    println!();
    println!("WARNING: This file contains your private keys.");
    println!("Store it securely and delete it after importing.");

    Ok(())
}

pub fn import(file: &str, force: bool) -> Result<(), String> {
    let content =
        std::fs::read_to_string(file).map_err(|e| format!("Failed to read {}: {}", file, e))?;
    let profile: ProfileExport =
        toml::from_str(&content).map_err(|e| format!("Failed to parse profile: {}", e))?;

    if profile.version != 1 {
        return Err(format!(
            "Unsupported profile version: {}. Update your client.",
            profile.version
        ));
    }

    let mut imported = 0;
    let mut skipped = 0;

    for entry in &profile.servers {
        // Validate the identity key
        let key_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &entry.identity_key,
        )
        .map_err(|e| format!("Invalid identity key for {}: {}", entry.address, e))?;
        if key_bytes.len() != 64 {
            return Err(format!(
                "Invalid identity key length for {} (expected 64 bytes, got {})",
                entry.address,
                key_bytes.len()
            ));
        }

        let srv_dir = config::server_dir(&entry.address);
        if srv_dir.exists() {
            if !force {
                eprint!(
                    "Server {} already configured. Overwrite? [y/N] ",
                    entry.address
                );
                io::stderr().flush().ok();
                let mut answer = String::new();
                io::stdin()
                    .read_line(&mut answer)
                    .map_err(|e| format!("Failed to read input: {}", e))?;
                if !answer.trim().eq_ignore_ascii_case("y") {
                    skipped += 1;
                    continue;
                }
            }
        }

        // Create server directory
        std::fs::create_dir_all(&srv_dir)
            .map_err(|e| format!("Failed to create directory for {}: {}", entry.address, e))?;

        // Write identity key
        let key_path = config::server_identity_path(&entry.address);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&key_path)
                .and_then(|mut f| f.write_all(entry.identity_key.as_bytes()))
                .map_err(|e| format!("Failed to write identity for {}: {}", entry.address, e))?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&key_path, &entry.identity_key)
                .map_err(|e| format!("Failed to write identity for {}: {}", entry.address, e))?;
        }

        // Write server.toml
        let srv_cfg = config::ServerConfig {
            server: entry.address.clone(),
            username: entry.username.clone(),
            server_name: entry.server_name.clone(),
        };
        srv_cfg.save()?;

        imported += 1;
    }

    // Write global config
    if force || !config::config_path().exists() {
        let global = config::GlobalConfig {
            socks_proxy: profile.global.socks_proxy,
            editor: profile.global.editor,
            reply_context: profile.global.reply_context,
            default_server: profile.global.default_server,
            last_server: None,
        };
        global.save()?;
    }

    println!("Imported {} server(s).", imported);
    if skipped > 0 {
        println!("Skipped {} existing server(s).", skipped);
    }
    println!();
    println!("Run `agora servers` to verify.");

    Ok(())
}
