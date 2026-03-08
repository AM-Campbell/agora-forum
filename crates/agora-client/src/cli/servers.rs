use crate::config;

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
