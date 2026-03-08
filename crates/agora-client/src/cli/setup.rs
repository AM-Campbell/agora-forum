use std::io::{self, Write};
use std::net::TcpStream;
use std::time::Duration;

use crate::api::ApiClient;
use crate::config::{GlobalConfig, ServerConfig};
use crate::identity::Identity;

pub async fn run(override_server: Option<&str>) -> Result<(), String> {
    println!("\n  AGORA — Join a Forum\n");

    // Step 1: Detect SOCKS5 proxy (auto-detect or prompt)
    let socks_proxy = detect_socks_proxy();
    match &socks_proxy {
        Some(proxy) => println!("  Checking for Tor... found (SOCKS5 proxy at {})\n", proxy),
        None => {
            println!("  Checking for Tor... not found\n");
            println!("  Tor is required. Install it with:");
            println!("    Ubuntu/Debian:  sudo apt install tor && sudo systemctl start tor");
            println!("    Arch:           sudo pacman -S tor && sudo systemctl start tor");
            println!("    macOS:          brew install tor && brew services start tor");
            println!("\n  Then re-run: agora setup");
            // Don't bail — allow setup for local testing without Tor
        }
    }

    // Step 2: Server address
    let server = if let Some(s) = override_server {
        println!("  Server address: {}", s);
        s.to_string()
    } else {
        print!("  Server address: ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let mut input = input.trim().to_string();
        if input.is_empty() {
            return Err("Server address is required. It looks like: http://xxxxx.onion".to_string());
        }
        // Auto-prepend http:// if they pasted a bare address
        if !input.starts_with("http://") && !input.starts_with("https://") {
            if input.contains(".onion") || input.contains("localhost") || input.contains("127.0.0.1") {
                input = format!("http://{}", input);
                println!("  (added http:// — using {})", input);
            } else {
                return Err("Server address should look like: http://xxxxx.onion".to_string());
            }
        }
        input
    };

    // Check if this server is already configured
    if crate::config::server_config_path(&server).exists() {
        print!("  Server already configured. Overwrite? [y/N] ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("  Aborted.");
            return Ok(());
        }
    }

    // Step 3: Invite code
    print!("  Invite code: ");
    io::stdout().flush().ok();
    let mut invite_code = String::new();
    io::stdin().read_line(&mut invite_code).ok();
    let invite_code = invite_code.trim().to_string();
    if invite_code.is_empty() {
        return Err("Invite code is required. Ask the person who invited you for it.".to_string());
    }

    // Step 4: Username
    print!("  Choose a username: ");
    io::stdout().flush().ok();
    let mut username = String::new();
    io::stdin().read_line(&mut username).ok();
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("Username is required.".to_string());
    }
    // Validate locally before hitting the server
    if username.len() < 3 || username.len() > 20 {
        return Err("Username must be 3-20 characters.".to_string());
    }
    if !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Username can only contain letters, numbers, and underscores.".to_string());
    }

    // Step 5: Generate keypair
    print!("\n  Generating keypair... ");
    io::stdout().flush().ok();
    let identity = Identity::generate_for(&server)?;
    println!("done");

    // Step 6: Register with server
    print!("  Registering with server... ");
    io::stdout().flush().ok();

    let socks = socks_proxy.unwrap_or_else(|| "127.0.0.1:9050".to_string());

    let api = if server.contains(".onion") {
        ApiClient::new(&server, &socks, identity)?
    } else {
        ApiClient::new_direct(&server, identity)?
    };

    let resp = api.register(&username, &invite_code).await?;
    println!("done");

    // Fetch server name (best-effort, don't fail setup if it doesn't work)
    let server_name = match api.get_version().await {
        Ok(v) => v.server_name,
        Err(_) => None,
    };

    // Step 7: Save configs
    let srv_config = ServerConfig {
        server: server.clone(),
        username: username.clone(),
        server_name: server_name.clone(),
    };
    srv_config.save()?;

    // Load existing global config or create new one
    let mut global = GlobalConfig::load_or_default();
    global.socks_proxy = socks;
    // Set as default if no default exists
    if global.default_server.is_none() {
        global.default_server = Some(server);
    }
    global.save()?;

    let display_name = server_name.as_deref().unwrap_or("AGORA");
    println!("\n  Welcome to {}, {}!", display_name, resp.username);
    println!("  Run 'agora' to open the forum.");

    Ok(())
}

/// Try to connect to common Tor SOCKS5 proxy ports.
fn detect_socks_proxy() -> Option<String> {
    let timeout = Duration::from_secs(1);
    for port in [9050, 9150] {
        let addr = format!("127.0.0.1:{}", port);
        if TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            timeout,
        )
        .is_ok()
        {
            return Some(addr);
        }
    }
    None
}
