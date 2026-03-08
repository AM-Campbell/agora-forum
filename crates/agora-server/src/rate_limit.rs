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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_normal_request_passes() {
        let mut limiter = RateLimiter::default();
        assert!(limiter.check("user1", false).is_ok());
    }

    #[test]
    fn check_general_limit_exceeded() {
        let mut limiter = RateLimiter::default();
        for _ in 0..GENERAL_LIMIT {
            limiter.check("user1", false).unwrap();
        }
        let result = limiter.check("user1", false);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Rate limit exceeded. Try again later.");
    }

    #[test]
    fn check_post_limit_separate_from_general() {
        let mut limiter = RateLimiter::default();
        // Exhaust post limit
        for _ in 0..POST_LIMIT {
            limiter.check("user1", true).unwrap();
        }
        // Post should fail
        let result = limiter.check("user1", true);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Post rate limit exceeded. Try again later."
        );
        // Non-post general request should still pass (we've made 10 + 1 general requests total)
        assert!(limiter.check("user1", false).is_ok());
    }

    #[test]
    fn check_register_normal_passes() {
        let mut limiter = RateLimiter::default();
        assert!(limiter.check_register().is_ok());
    }

    #[test]
    fn check_register_limit_exceeded() {
        let mut limiter = RateLimiter::default();
        for _ in 0..REGISTER_LIMIT {
            limiter.check_register().unwrap();
        }
        let result = limiter.check_register();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Too many registration attempts. Try again later."
        );
    }

    #[test]
    fn cleanup_removes_old_entries() {
        let mut limiter = RateLimiter::default();
        // Manually insert an old record
        let old_timestamp = now_secs() - GENERAL_WINDOW_SECS - 1;
        limiter.windows.insert(
            "old_user".to_string(),
            UserWindow {
                general: vec![RequestRecord {
                    timestamp: old_timestamp,
                }],
                posts: vec![RequestRecord {
                    timestamp: old_timestamp,
                }],
            },
        );
        // Also insert a current record
        limiter.check("current_user", false).unwrap();

        limiter.cleanup();

        // Old user should be removed entirely
        assert!(!limiter.windows.contains_key("old_user"));
        // Current user should remain
        assert!(limiter.windows.contains_key("current_user"));
    }

    #[test]
    fn cleanup_removes_old_register_attempts() {
        let mut limiter = RateLimiter::default();
        let old_timestamp = now_secs() - REGISTER_WINDOW_SECS - 1;
        limiter.register_attempts.push(RequestRecord {
            timestamp: old_timestamp,
        });
        limiter.check_register().unwrap();

        limiter.cleanup();

        // Only the recent attempt should remain
        assert_eq!(limiter.register_attempts.len(), 1);
    }

    #[test]
    fn different_users_have_independent_limits() {
        let mut limiter = RateLimiter::default();
        for _ in 0..GENERAL_LIMIT {
            limiter.check("user1", false).unwrap();
        }
        // user1 is exhausted
        assert!(limiter.check("user1", false).is_err());
        // user2 should be fine
        assert!(limiter.check("user2", false).is_ok());
    }
}
