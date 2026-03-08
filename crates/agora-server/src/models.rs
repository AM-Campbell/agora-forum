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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_new_from_str() {
        let body = ErrorBody::new("something went wrong");
        assert_eq!(body.error, "something went wrong");
    }

    #[test]
    fn error_body_new_from_string() {
        let body = ErrorBody::new(String::from("another error"));
        assert_eq!(body.error, "another error");
    }

    #[test]
    fn error_body_serializes_to_json() {
        let body = ErrorBody::new("test error");
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"error\":\"test error\""));
    }
}
