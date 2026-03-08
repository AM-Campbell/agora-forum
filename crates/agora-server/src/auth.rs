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

const MAX_TIMESTAMP_DRIFT: i64 = 60;

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
    if (now - timestamp).abs() > MAX_TIMESTAMP_DRIFT {
        warn!("Auth failed: timestamp drift");
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody::new("Timestamp too old")),
        )
            .into_response();
    }

    // Decode public key
    let engine = base64::engine::general_purpose::STANDARD;
    let pubkey_bytes = match engine.decode(&pubkey_b64) {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Invalid public key encoding")),
            )
                .into_response();
        }
    };

    let pubkey_array: [u8; 32] = match pubkey_bytes.as_slice().try_into() {
        Ok(a) => a,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Invalid public key length")),
            )
                .into_response();
        }
    };
    let verifying_key = match VerifyingKey::from_bytes(&pubkey_array) {
        Ok(k) => k,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Invalid public key")),
            )
                .into_response();
        }
    };

    // Decode signature
    let sig_bytes = match engine.decode(&sig_b64) {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Invalid signature encoding")),
            )
                .into_response();
        }
    };

    let sig_array: [u8; 64] = match sig_bytes.as_slice().try_into() {
        Ok(a) => a,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorBody::new("Invalid signature length")),
            )
                .into_response();
        }
    };
    let signature = Signature::from_bytes(&sig_array);

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
    let signing_string = format!("{}\n{}\n{}\n{}", method, path, timestamp_str, body_str);

    if verifying_key.verify(signing_string.as_bytes(), &signature).is_err() {
        warn!(path = %path, "Auth failed: invalid signature");
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
