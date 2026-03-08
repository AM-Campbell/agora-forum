use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey, Verifier};

use tracing::warn;

use crate::models::ErrorBody;
use crate::AppState;

pub const MAX_TIMESTAMP_DRIFT: i64 = 60;

/// Validate that a timestamp is within acceptable drift of `now`.
pub fn validate_timestamp(timestamp: i64, now: i64) -> Result<(), &'static str> {
    if (now - timestamp).abs() > MAX_TIMESTAMP_DRIFT {
        Err("Timestamp too old")
    } else {
        Ok(())
    }
}

/// Decode a base64 string into a fixed-size byte array.
pub fn decode_base64_fixed<const N: usize>(s: &str) -> Result<[u8; N], &'static str> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| "Invalid base64 encoding")?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| "Invalid decoded length")
}

/// Build the signing string from request components.
pub fn build_signing_string(method: &str, path: &str, timestamp: &str, body: &str) -> String {
    format!("{}\n{}\n{}\n{}", method, path, timestamp, body)
}

/// Verify an ed25519 signature over a message.
pub fn verify_signature(
    pubkey_bytes: &[u8; 32],
    message: &[u8],
    sig_bytes: &[u8; 64],
) -> Result<(), &'static str> {
    let verifying_key =
        VerifyingKey::from_bytes(pubkey_bytes).map_err(|_| "Invalid public key")?;
    let signature = Signature::from_bytes(sig_bytes);
    verifying_key
        .verify(message, &signature)
        .map_err(|_| "Invalid signature")
}

/// Authenticated user info attached to request extensions.
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user_id: i64,
    #[allow(dead_code)]
    pub username: String,
}

/// Middleware that verifies ed25519 signature auth on requests.
pub async fn auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let pubkey_header = request
        .headers()
        .get("X-Agora-PublicKey")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let timestamp_header = request
        .headers()
        .get("X-Agora-Timestamp")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let signature_header = request
        .headers()
        .get("X-Agora-Signature")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let (pubkey_b64, timestamp_str, sig_b64) =
        match (pubkey_header, timestamp_header, signature_header) {
            (Some(p), Some(t), Some(s)) => (p, t, s),
            _ => {
                let path = request.uri().path().to_string();
                warn!(path = %path, "Auth failed: missing headers");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorBody::new("Missing authentication headers")),
                )
                    .into_response();
            }
        };

    // Parse timestamp
    let timestamp: i64 = match timestamp_str.parse() {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Invalid timestamp")),
            )
                .into_response();
        }
    };

    let now = chrono::Utc::now().timestamp();
    if let Err(msg) = validate_timestamp(timestamp, now) {
        warn!("Auth failed: timestamp drift");
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody::new(msg)),
        )
            .into_response();
    }

    // Decode public key
    let pubkey_array: [u8; 32] = match decode_base64_fixed(&pubkey_b64) {
        Ok(a) => a,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Invalid public key encoding")),
            )
                .into_response();
        }
    };

    // Decode signature
    let sig_array: [u8; 64] = match decode_base64_fixed(&sig_b64) {
        Ok(a) => a,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Invalid signature encoding")),
            )
                .into_response();
        }
    };

    // Reconstruct signing string — extract method/path+query before consuming request
    let method = request.method().to_string();
    let path = request
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());

    // We need the body for verification but also need to forward it
    let (parts, body) = request.into_parts();
    // Allow up to 8 MB for base64-encoded file uploads (5 MB file = ~7 MB base64 + JSON)
    let body_bytes = match axum::body::to_bytes(body, 8 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody::new("Failed to read request body")),
            )
                .into_response();
        }
    };

    let body_str = String::from_utf8_lossy(&body_bytes);
    let signing_string = build_signing_string(&method, &path, &timestamp_str, &body_str);

    if let Err(msg) = verify_signature(&pubkey_array, signing_string.as_bytes(), &sig_array) {
        warn!(path = %path, "Auth failed: {}", msg);
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody::new("Invalid signature")),
        )
            .into_response();
    }

    // Look up user by public key
    let user = {
        let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
        conn.query_row(
            "SELECT id, username FROM users WHERE public_key = ?1",
            [&pubkey_b64],
            |row| {
                Ok(AuthUser {
                    user_id: row.get(0)?,
                    username: row.get(1)?,
                })
            },
        )
        .ok()
    };

    let user = match user {
        Some(u) => u,
        None => {
            warn!("Auth failed: unknown public key");
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Unknown public key")),
            )
                .into_response();
        }
    };

    // Check if user is banned
    {
        let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
        let is_banned: bool = conn
            .query_row(
                "SELECT COALESCE(is_banned, 0) FROM users WHERE id = ?1",
                [user.user_id],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if is_banned {
            warn!(user = %user.username, "Auth rejected: user is banned");
            return (
                StatusCode::FORBIDDEN,
                Json(ErrorBody::new("You are banned")),
            )
                .into_response();
        }
    }

    // Update last_seen_at
    {
        let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE users SET last_seen_at = datetime('now') WHERE id = ?1",
            [user.user_id],
        )
        .ok();
    }

    // Rate limit check
    let is_post = method == "POST"
        && (path.contains("/threads") || path.contains("/posts") || path.starts_with("/dm"));
    {
        let mut limiter = state.rate_limiter.lock().await;
        if let Err(msg) = limiter.check(&user.user_id.to_string(), is_post) {
            warn!(user = %user.username, "Rate limit exceeded");
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorBody::new(msg)),
            )
                .into_response();
        }
    }

    // Re-assemble request with body
    let mut request = Request::from_parts(parts, Body::from(body_bytes));
    request.extensions_mut().insert(user);

    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_timestamp ---

    #[test]
    fn timestamp_valid_exact_now() {
        assert!(validate_timestamp(1000, 1000).is_ok());
    }

    #[test]
    fn timestamp_valid_within_drift() {
        assert!(validate_timestamp(1000, 1060).is_ok()); // exactly at boundary
        assert!(validate_timestamp(1060, 1000).is_ok()); // future within drift
        assert!(validate_timestamp(1000, 1030).is_ok()); // 30s drift
    }

    #[test]
    fn timestamp_invalid_too_old() {
        assert_eq!(validate_timestamp(1000, 1061), Err("Timestamp too old"));
    }

    #[test]
    fn timestamp_invalid_too_far_future() {
        assert_eq!(validate_timestamp(1061, 1000), Err("Timestamp too old"));
    }

    // --- decode_base64_fixed ---

    #[test]
    fn decode_base64_32_valid() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([0xABu8; 32]);
        let result: [u8; 32] = decode_base64_fixed(&encoded).unwrap();
        assert_eq!(result, [0xAB; 32]);
    }

    #[test]
    fn decode_base64_64_valid() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([0xCDu8; 64]);
        let result: [u8; 64] = decode_base64_fixed(&encoded).unwrap();
        assert_eq!(result, [0xCD; 64]);
    }

    #[test]
    fn decode_base64_invalid_encoding() {
        let result: Result<[u8; 32], _> = decode_base64_fixed("not-valid!!!");
        assert_eq!(result, Err("Invalid base64 encoding"));
    }

    #[test]
    fn decode_base64_wrong_length() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        let result: Result<[u8; 32], _> = decode_base64_fixed(&encoded);
        assert_eq!(result, Err("Invalid decoded length"));
    }

    // --- build_signing_string ---

    #[test]
    fn signing_string_format() {
        let result = build_signing_string("GET", "/boards", "12345", "");
        assert_eq!(result, "GET\n/boards\n12345\n");
    }

    #[test]
    fn signing_string_with_body() {
        let result = build_signing_string("POST", "/threads/1/posts", "99999", "{\"body\":\"hi\"}");
        assert_eq!(result, "POST\n/threads/1/posts\n99999\n{\"body\":\"hi\"}");
    }

    #[test]
    fn signing_string_with_query() {
        let result = build_signing_string("GET", "/boards/general?page=2", "12345", "");
        assert_eq!(result, "GET\n/boards/general?page=2\n12345\n");
    }

    // --- verify_signature ---

    #[test]
    fn verify_signature_valid() {
        use ed25519_dalek::{SigningKey, Signer};
        let signing_key = SigningKey::from_bytes(&[1u8; 32]);
        let message = b"test message";
        let sig = signing_key.sign(message);
        let pubkey = signing_key.verifying_key().to_bytes();
        assert!(verify_signature(&pubkey, message, &sig.to_bytes()).is_ok());
    }

    #[test]
    fn verify_signature_wrong_message() {
        use ed25519_dalek::{SigningKey, Signer};
        let signing_key = SigningKey::from_bytes(&[2u8; 32]);
        let sig = signing_key.sign(b"correct message");
        let pubkey = signing_key.verifying_key().to_bytes();
        assert_eq!(
            verify_signature(&pubkey, b"wrong message", &sig.to_bytes()),
            Err("Invalid signature")
        );
    }

    #[test]
    fn verify_signature_wrong_key() {
        use ed25519_dalek::{SigningKey, Signer};
        let signing_key = SigningKey::from_bytes(&[3u8; 32]);
        let other_key = SigningKey::from_bytes(&[4u8; 32]);
        let message = b"test";
        let sig = signing_key.sign(message);
        let wrong_pubkey = other_key.verifying_key().to_bytes();
        assert_eq!(
            verify_signature(&wrong_pubkey, message, &sig.to_bytes()),
            Err("Invalid signature")
        );
    }

    #[test]
    fn verify_signature_full_auth_flow() {
        // Simulate the full auth signing flow
        use ed25519_dalek::{SigningKey, Signer};
        let signing_key = SigningKey::from_bytes(&[5u8; 32]);
        let pubkey = signing_key.verifying_key().to_bytes();

        let signing_string = build_signing_string("POST", "/boards/general/threads", "1700000000", "{\"title\":\"Hello\",\"body\":\"World\"}");
        let sig = signing_key.sign(signing_string.as_bytes());

        assert!(verify_signature(&pubkey, signing_string.as_bytes(), &sig.to_bytes()).is_ok());
    }
}
