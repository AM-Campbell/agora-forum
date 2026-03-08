use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

// ── Global config (~/.agora/config.toml) ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_socks_proxy")]
    pub socks_proxy: String,
    pub editor: Option<String>,
    #[serde(default = "default_reply_context")]
    pub reply_context: usize,
    pub default_server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_server: Option<String>,
}

fn default_socks_proxy() -> String {
    "127.0.0.1:9050".to_string()
}

fn default_reply_context() -> usize {
    3
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            socks_proxy: default_socks_proxy(),
            editor: None,
            reply_context: default_reply_context(),
            default_server: None,
            last_server: None,
        }
    }
}

impl GlobalConfig {
    pub fn load() -> Result<Self, String> {
        let path = config_path();
        if !path.exists() {
            return Err(format!(
                "Config file not found at {}. Run `agora setup` to create it.",
                path.display()
            ));
        }
        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read config: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
    }

    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        let content =
            toml::to_string_pretty(self).map_err(|e| format!("Failed to serialize config: {}", e))?;
        std::fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))
    }
}

// ── Per-server config (~/.agora/servers/<hash>/server.toml) ─────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub server: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
}

impl ServerConfig {
    pub fn load(server_addr: &str) -> Result<Self, String> {
        let path = server_config_path(server_addr);
        if !path.exists() {
            return Err(format!(
                "No config found for server {}. Run `agora setup` to join.",
                server_addr
            ));
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read server config: {}", e))?;
        toml::from_str(&content).map_err(|e| format!("Failed to parse server config: {}", e))
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = server_dir(&self.server);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create server directory: {}", e))?;
        let path = dir.join("server.toml");
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize server config: {}", e))?;
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write server config: {}", e))
    }
}

// ── Path helpers ────────────────────────────────────────────────

pub fn agora_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not determine home directory")
        .join(".agora")
}

pub fn config_path() -> PathBuf {
    agora_dir().join("config.toml")
}

pub fn servers_dir() -> PathBuf {
    agora_dir().join("servers")
}

fn server_hash(server_addr: &str) -> String {
    let hash = Sha256::digest(server_addr.as_bytes());
    hex::encode(&hash[..8])
}

pub fn server_dir(server_addr: &str) -> PathBuf {
    servers_dir().join(server_hash(server_addr))
}

pub fn server_config_path(server_addr: &str) -> PathBuf {
    server_dir(server_addr).join("server.toml")
}

pub fn server_identity_path(server_addr: &str) -> PathBuf {
    server_dir(server_addr).join("identity.key")
}

pub fn server_cache_path(server_addr: &str) -> PathBuf {
    server_dir(server_addr).join("cache.db")
}

pub fn drafts_dir() -> PathBuf {
    agora_dir().join("drafts")
}

// ── Server enumeration ──────────────────────────────────────────

pub fn list_servers() -> Vec<ServerConfig> {
    let dir = servers_dir();
    if !dir.exists() {
        return Vec::new();
    }
    let mut servers = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path().join("server.toml");
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(cfg) = toml::from_str::<ServerConfig>(&content) {
                        servers.push(cfg);
                    }
                }
            }
        }
    }
    servers
}

// ── Editor helper ───────────────────────────────────────────────

pub fn get_editor() -> String {
    if let Ok(config) = GlobalConfig::load() {
        if let Some(editor) = config.editor {
            return editor;
        }
    }
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string())
}

// ── Default server management ───────────────────────────────────

pub fn set_default_server(server_addr: &str) -> Result<(), String> {
    // Verify this server is actually configured
    let path = server_config_path(server_addr);
    if !path.exists() {
        return Err(format!(
            "No config found for server {}. Run `agora setup` to join it first.",
            server_addr
        ));
    }
    let mut global = GlobalConfig::load_or_default();
    global.default_server = Some(server_addr.to_string());
    global.save()
}

pub fn set_last_server(server_addr: &str) -> Result<(), String> {
    let mut global = GlobalConfig::load_or_default();
    global.last_server = Some(server_addr.to_string());
    global.save()
}
