/// Connection state for the TUI status bar.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Online,
    Offline,
    Connecting,
}

impl ConnectionState {
    pub fn label(&self) -> &str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Connecting => "connecting",
        }
    }
}
