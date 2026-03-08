use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

const GENERAL_LIMIT: u64 = 120;
const GENERAL_WINDOW_SECS: u64 = 60;
const POST_LIMIT: u64 = 10;
const POST_WINDOW_SECS: u64 = 60;
const REGISTER_LIMIT: u64 = 10;
const REGISTER_WINDOW_SECS: u64 = 60;
const CLEANUP_INTERVAL_SECS: u64 = 300;

#[derive(Debug, Clone)]
struct RequestRecord {
    timestamp: u64,
}

#[derive(Debug, Default)]
struct UserWindow {
    general: Vec<RequestRecord>,
    posts: Vec<RequestRecord>,
}

pub type RateLimiterState = Arc<Mutex<RateLimiter>>;

#[derive(Debug, Default)]
pub struct RateLimiter {
    windows: HashMap<String, UserWindow>,
    register_attempts: Vec<RequestRecord>,
}

impl RateLimiter {
    pub fn new() -> RateLimiterState {
        Arc::new(Mutex::new(Self {
            windows: HashMap::new(),
            register_attempts: Vec::new(),
        }))
    }

    /// Check and record a request. Returns Ok(()) if allowed, Err(message) if rate limited.
    pub fn check(&mut self, user_id: &str, is_post: bool) -> Result<(), &'static str> {
        let now = now_secs();
        let window = self.windows.entry(user_id.to_string()).or_default();

        // Prune expired general entries
        window
            .general
            .retain(|r| now - r.timestamp < GENERAL_WINDOW_SECS);

        if window.general.len() as u64 >= GENERAL_LIMIT {
            return Err("Rate limit exceeded. Try again later.");
        }

        if is_post {
            window
                .posts
                .retain(|r| now - r.timestamp < POST_WINDOW_SECS);

            if window.posts.len() as u64 >= POST_LIMIT {
                return Err("Post rate limit exceeded. Try again later.");
            }

            window.posts.push(RequestRecord { timestamp: now });
        }

        window.general.push(RequestRecord { timestamp: now });
        Ok(())
    }

    /// Check global registration rate limit.
    pub fn check_register(&mut self) -> Result<(), &'static str> {
        let now = now_secs();
        self.register_attempts
            .retain(|r| now - r.timestamp < REGISTER_WINDOW_SECS);
        if self.register_attempts.len() as u64 >= REGISTER_LIMIT {
            return Err("Too many registration attempts. Try again later.");
        }
        self.register_attempts.push(RequestRecord { timestamp: now });
        Ok(())
    }

    fn cleanup(&mut self) {
        let now = now_secs();
        self.windows.retain(|_, w| {
            w.general
                .retain(|r| now - r.timestamp < GENERAL_WINDOW_SECS);
            w.posts
                .retain(|r| now - r.timestamp < POST_WINDOW_SECS);
            !w.general.is_empty() || !w.posts.is_empty()
        });
        self.register_attempts
            .retain(|r| now - r.timestamp < REGISTER_WINDOW_SECS);
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn spawn_cleanup_task(limiter: RateLimiterState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(CLEANUP_INTERVAL_SECS)).await;
            limiter.lock().await.cleanup();
        }
    });
}
