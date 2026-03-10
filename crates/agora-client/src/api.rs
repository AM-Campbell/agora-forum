use reqwest::Client;

use crate::identity::Identity;
use agora_common::*;

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    identity: Identity,
}

impl ApiClient {
    pub fn new(server: &str, socks_proxy: &str, identity: Identity) -> Result<Self, String> {
        let proxy = reqwest::Proxy::all(format!("socks5h://{}", socks_proxy))
            .map_err(|e| format!("Invalid proxy config: {}", e))?;

        let client = Client::builder()
            .proxy(proxy)
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            client,
            base_url: server.trim_end_matches('/').to_string(),
            identity,
        })
    }

    /// Create a client that connects directly (no SOCKS proxy), for local testing.
    pub fn new_direct(base_url: &str, identity: Identity) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            identity,
        })
    }

    /// Max response body size (10 MB) to prevent malicious servers from OOM-killing the client.
    const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Read a response body with a size limit, then deserialize as JSON.
    async fn limited_json<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T, String> {
        let bytes = resp.bytes().await.map_err(|e| format!("Read error: {}", e))?;
        if bytes.len() > Self::MAX_RESPONSE_SIZE {
            return Err("Server response too large".to_string());
        }
        serde_json::from_slice(&bytes).map_err(|e| format!("Parse error: {}", e))
    }

    fn parse_error(status: reqwest::StatusCode, body: &str) -> String {
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(body) {
            err.error
        } else {
            format!("Server error ({}): {}", status, body)
        }
    }

    fn auth_headers(&self, method: &str, path: &str, body: &str) -> Vec<(String, String)> {
        let (timestamp, signature) = self.identity.sign_request(method, path, body);
        vec![
            ("X-Agora-PublicKey".to_string(), self.identity.public_key_base64()),
            ("X-Agora-Timestamp".to_string(), timestamp),
            ("X-Agora-Signature".to_string(), signature),
        ]
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub async fn register(
        &self,
        username: &str,
        invite_code: &str,
    ) -> Result<RegisterResponse, String> {
        let req = RegisterRequest {
            username: username.to_string(),
            public_key: self.identity.public_key_base64(),
            invite_code: invite_code.to_string(),
        };

        let resp = self
            .client
            .post(self.url("/register"))
            .json(&req)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            Self::limited_json(resp).await
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(Self::parse_error(status, &body))
        }
    }

    async fn authed_get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let headers = self.auth_headers("GET", path, "");
        let mut req = self.client.get(self.url(path));
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            Self::limited_json(resp).await
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(Self::parse_error(status, &body))
        }
    }

    async fn authed_post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, String> {
        let body_json = serde_json::to_string(body).map_err(|e| format!("Serialize error: {}", e))?;
        let headers = self.auth_headers("POST", path, &body_json);
        let mut req = self
            .client
            .post(self.url(path))
            .header("Content-Type", "application/json")
            .body(body_json);
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            Self::limited_json(resp).await
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(Self::parse_error(status, &body))
        }
    }

    async fn authed_put<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T, String> {
        let body_json = serde_json::to_string(body).map_err(|e| format!("Serialize error: {}", e))?;
        let headers = self.auth_headers("PUT", path, &body_json);
        let mut req = self
            .client
            .put(self.url(path))
            .header("Content-Type", "application/json")
            .body(body_json);
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            Self::limited_json(resp).await
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(Self::parse_error(status, &body))
        }
    }

    async fn authed_delete<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let headers = self.auth_headers("DELETE", path, "");
        let mut req = self.client.delete(self.url(path));
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            Self::limited_json(resp).await
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(Self::parse_error(status, &body))
        }
    }

    /// Get raw bytes from an authenticated GET (for attachment downloads).
    async fn authed_get_bytes(&self, path: &str) -> Result<(Vec<u8>, String, String), String> {
        let headers = self.auth_headers("GET", path, "");
        let mut req = self.client.get(self.url(path));
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();
            let filename = resp
                .headers()
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| {
                    v.split("filename=").nth(1).map(|s| s.trim_matches('"').to_string())
                })
                .unwrap_or_else(|| "download".to_string());
            let bytes = resp.bytes().await.map_err(|e| format!("Read error: {}", e))?;
            Ok((bytes.to_vec(), content_type, filename))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(Self::parse_error(status, &body))
        }
    }

    // --- Version ---

    pub async fn get_version(&self) -> Result<VersionResponse, String> {
        let resp = self
            .client
            .get(self.url("/version"))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            Self::limited_json(resp).await
        } else {
            Err("Server does not support version endpoint".to_string())
        }
    }

    // --- Boards ---

    pub async fn get_boards(&self) -> Result<BoardListResponse, String> {
        self.authed_get("/boards").await
    }

    pub async fn get_threads(&self, slug: &str, page: i64) -> Result<ThreadListResponse, String> {
        self.authed_get(&format!("/boards/{}?page={}", slug, page))
            .await
    }

    pub async fn get_thread(&self, thread_id: i64, page: i64) -> Result<ThreadViewResponse, String> {
        self.authed_get(&format!("/threads/{}?page={}", thread_id, page))
            .await
    }

    pub async fn create_thread(
        &self,
        slug: &str,
        title: &str,
        body: &str,
    ) -> Result<CreateThreadResponse, String> {
        let req = CreateThreadRequest {
            title: title.to_string(),
            body: body.to_string(),
        };
        self.authed_post(&format!("/boards/{}/threads", slug), &req)
            .await
    }

    pub async fn create_post(
        &self,
        thread_id: i64,
        body: &str,
    ) -> Result<CreatePostResponse, String> {
        let req = CreatePostRequest {
            body: body.to_string(),
            parent_post_id: None,
        };
        self.authed_post(&format!("/threads/{}/posts", thread_id), &req)
            .await
    }

    pub async fn create_post_reply(
        &self,
        thread_id: i64,
        body: &str,
        parent_post_id: i64,
    ) -> Result<CreatePostResponse, String> {
        let req = CreatePostRequest {
            body: body.to_string(),
            parent_post_id: Some(parent_post_id),
        };
        self.authed_post(&format!("/threads/{}/posts", thread_id), &req)
            .await
    }

    // --- Post editing ---

    pub async fn edit_post(
        &self,
        thread_id: i64,
        post_id: i64,
        body: &str,
    ) -> Result<EditPostResponse, String> {
        let req = EditPostRequest {
            body: body.to_string(),
        };
        self.authed_put(&format!("/threads/{}/posts/{}", thread_id, post_id), &req)
            .await
    }

    pub async fn post_history(
        &self,
        thread_id: i64,
        post_id: i64,
    ) -> Result<PostHistoryResponse, String> {
        self.authed_get(&format!("/threads/{}/posts/{}/history", thread_id, post_id))
            .await
    }

    // --- Moderation ---

    pub async fn mod_thread(
        &self,
        thread_id: i64,
        action: &str,
    ) -> Result<ModActionResponse, String> {
        let req = ModActionRequest {
            action: action.to_string(),
            target_user: None,
            role: None,
        };
        self.authed_post(&format!("/threads/{}/mod", thread_id), &req)
            .await
    }

    pub async fn mod_post(
        &self,
        thread_id: i64,
        post_id: i64,
        action: &str,
    ) -> Result<ModActionResponse, String> {
        let req = ModActionRequest {
            action: action.to_string(),
            target_user: None,
            role: None,
        };
        self.authed_post(&format!("/threads/{}/posts/{}/mod", thread_id, post_id), &req)
            .await
    }

    pub async fn mod_user(
        &self,
        username: &str,
        action: &str,
        role: Option<&str>,
    ) -> Result<ModActionResponse, String> {
        let req = ModActionRequest {
            action: action.to_string(),
            target_user: None,
            role: role.map(|s| s.to_string()),
        };
        self.authed_post(&format!("/users/{}/mod", username), &req)
            .await
    }

    async fn authed_post_empty<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, String> {
        let body_json = "{}";
        let headers = self.auth_headers("POST", path, body_json);
        let mut req = self
            .client
            .post(self.url(path))
            .header("Content-Type", "application/json")
            .body(body_json.to_string());
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            Self::limited_json(resp).await
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(Self::parse_error(status, &body))
        }
    }

    // --- Bookmarks ---

    pub async fn list_bookmarks(&self) -> Result<BookmarkListResponse, String> {
        self.authed_get("/bookmarks").await
    }

    pub async fn toggle_bookmark(&self, thread_id: i64) -> Result<BookmarkToggleResponse, String> {
        self.authed_post_empty(&format!("/bookmarks/{}", thread_id)).await
    }

    // --- Attachments ---

    pub async fn upload_attachment(
        &self,
        thread_id: i64,
        post_id: i64,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Result<UploadAttachmentResponse, String> {
        use base64::Engine;
        let data_base64 = base64::engine::general_purpose::STANDARD.encode(data);
        let req = UploadAttachmentRequest {
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            data_base64,
        };
        self.authed_post(
            &format!("/threads/{}/posts/{}/attachments", thread_id, post_id),
            &req,
        )
        .await
    }

    pub async fn download_attachment(
        &self,
        attachment_id: i64,
    ) -> Result<(Vec<u8>, String, String), String> {
        self.authed_get_bytes(&format!("/attachments/{}", attachment_id))
            .await
    }

    pub async fn delete_attachment(
        &self,
        attachment_id: i64,
    ) -> Result<ModActionResponse, String> {
        self.authed_delete(&format!("/attachments/{}", attachment_id))
            .await
    }

    // --- Invites ---

    pub async fn get_invites(&self) -> Result<InviteListResponse, String> {
        self.authed_get("/invites").await
    }

    pub async fn create_invite(&self) -> Result<InviteCreateResponse, String> {
        self.authed_post_empty("/invites").await
    }

    pub async fn get_me(&self) -> Result<MeResponse, String> {
        self.authed_get("/me").await
    }

    pub async fn check_connection(&self) -> Result<(), String> {
        let _ = self
            .client
            .get(self.url("/"))
            .send()
            .await
            .map_err(|e| format!("Connection failed: {}", e))?;
        Ok(())
    }

    // --- Member list ---

    pub async fn get_users(&self) -> Result<UserListResponse, String> {
        self.authed_get("/users").await
    }

    // --- Search ---

    pub async fn search(&self, query: &str, by: Option<&str>, page: i64) -> Result<SearchResponse, String> {
        let mut url = String::from("/search?");
        if !query.is_empty() {
            url.push_str(&format!("q={}&", urlencoding::encode(query)));
        }
        if let Some(user) = by {
            url.push_str(&format!("by={}&", urlencoding::encode(user)));
        }
        url.push_str(&format!("page={}", page));
        self.authed_get(&url).await
    }

    // --- Direct Messages ---

    pub async fn get_inbox(&self) -> Result<DmInboxResponse, String> {
        self.authed_get("/dm").await
    }

    pub async fn send_dm(
        &self,
        recipient: &str,
        ciphertext: &str,
        nonce: &str,
    ) -> Result<SendDmResponse, String> {
        let req = SendDmRequest {
            recipient: recipient.to_string(),
            ciphertext: ciphertext.to_string(),
            nonce: nonce.to_string(),
        };
        self.authed_post("/dm", &req).await
    }

    pub async fn get_conversation(
        &self,
        username: &str,
        page: i64,
    ) -> Result<DmConversationResponse, String> {
        self.authed_get(&format!("/dm/{}?page={}", username, page))
            .await
    }

    pub async fn get_user_public_key(
        &self,
        username: &str,
    ) -> Result<UserPublicKeyResponse, String> {
        self.authed_get(&format!("/users/{}/key", username)).await
    }

    // --- Reactions ---

    pub async fn react_post(
        &self,
        thread_id: i64,
        post_id: i64,
        reaction: &str,
    ) -> Result<ReactResponse, String> {
        let req = ReactRequest {
            reaction: reaction.to_string(),
        };
        self.authed_post(
            &format!("/threads/{}/posts/{}/react", thread_id, post_id),
            &req,
        )
        .await
    }

    // --- Bio ---

    pub async fn update_bio(&self, bio: &str) -> Result<UpdateBioResponse, String> {
        let req = UpdateBioRequest {
            bio: bio.to_string(),
        };
        self.authed_put("/me/bio", &req).await
    }

    // --- Mentions ---

    pub async fn get_mentions(&self, page: i64) -> Result<MentionsResponse, String> {
        self.authed_get(&format!("/mentions?page={}", page)).await
    }
}
