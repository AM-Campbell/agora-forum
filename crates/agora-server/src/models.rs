use serde::{Deserialize, Serialize};

/// Query params for paginated endpoints.
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub page: Option<i64>,
}

/// Query params for search endpoint.
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub by: Option<String>,
    pub page: Option<i64>,
}

/// Wrapper for JSON error responses.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: String,
}

impl ErrorBody {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            error: msg.into(),
        }
    }
}
