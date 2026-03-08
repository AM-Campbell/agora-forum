use crate::config;

pub fn update_address(old_addr: &str, new_addr: &str) -> Result<(), String> {
    let old_dir = config::server_dir(old_addr);
    if !old_dir.exists() {
        return Err(format!("No server configured for: {}", old_addr));
    }

    let new_dir = config::server_dir(new_addr);
    if new_dir.exists() {
        return Err(format!(
            "A server config already exists at: {}. Remove it first.",
            new_addr
        ));
    }

    // Update server.toml before renaming (so rename is the atomic commit point)
    let mut srv_cfg = config::ServerConfig::load(old_addr)?;
    srv_cfg.server = new_addr.to_string();
    let content = toml::to_string_pretty(&srv_cfg)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    std::fs::write(old_dir.join("server.toml"), content)
        .map_err(|e| format!("Failed to update server.toml: {}", e))?;

    // Rename directory
    std::fs::rename(&old_dir, &new_dir)
        .map_err(|e| format!("Failed to rename server directory: {}", e))?;

    // Update global config references
    let mut global = config::GlobalConfig::load_or_default();
    let mut changed = false;
    if global.default_server.as_deref() == Some(old_addr) {
        global.default_server = Some(new_addr.to_string());
        changed = true;
    }
    if global.last_server.as_deref() == Some(old_addr) {
        global.last_server = Some(new_addr.to_string());
        changed = true;
    }
    if changed {
        global.save()?;
    }

    println!("Server address updated:");
    println!("  {} → {}", old_addr, new_addr);
    Ok(())
}

pub fn remove(server_addr: &str) -> Result<(), String> {
    let srv_dir = config::server_dir(server_addr);
    if !srv_dir.exists() {
        return Err(format!("No server configured for: {}", server_addr));
    }

    // Confirm
    eprint!(
        "Remove server {}? This deletes your local identity and cache. [y/N] ",
        server_addr
    );
    use std::io::Write;
    std::io::stderr().flush().ok();
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|e| format!("Failed to read input: {}", e))?;
    if !answer.trim().eq_ignore_ascii_case("y") {
        println!("Cancelled.");
        return Ok(());
    }

    std::fs::remove_dir_all(&srv_dir)
        .map_err(|e| format!("Failed to remove server directory: {}", e))?;

    // Clean up global config references
    let mut global = config::GlobalConfig::load_or_default();
    let mut changed = false;
    if global.default_server.as_deref() == Some(server_addr) {
        global.default_server = None;
        changed = true;
    }
    if global.last_server.as_deref() == Some(server_addr) {
        global.last_server = None;
        changed = true;
    }
    if changed {
        global.save()?;
    }

    println!("Removed server: {}", server_addr);
    Ok(())
}

pub fn run() -> Result<(), String> {
    let servers = config::list_servers();
    let global = config::GlobalConfig::load_or_default();
    let default = global.default_server.as_deref().unwrap_or("");

    if servers.is_empty() {
        println!("No servers configured. Run `agora setup` to join one.");
        return Ok(());
    }

    println!("\n  {:3} {:<24} {:<42} {}", "", "Name", "Server", "Username");
    println!("  {}", "─".repeat(76));
    for srv in &servers {
        let marker = if srv.server == default { "*" } else { " " };
        let name = srv.server_name.as_deref().unwrap_or("—");
        println!("  {:<3} {:<24} {:<42} {}", marker, name, srv.server, srv.username);
    }
    println!("\n  (* = default)\n");

    Ok(())
}
