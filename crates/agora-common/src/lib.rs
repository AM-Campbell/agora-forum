use serde::{Deserialize, Serialize};

// --- Validation constants ---

pub const MAX_TITLE_LEN: usize = 200;
pub const MAX_BODY_LEN: usize = 10_000;
pub const MAX_USERNAME_LEN: usize = 20;
pub const MIN_USERNAME_LEN: usize = 3;
pub const MAX_BIO_LEN: usize = 200;

// --- Version ---

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionResponse {
    pub server_version: String,
    pub min_client_version: String,
    #[serde(default)]
    pub server_name: Option<String>,
}

// --- Registration ---

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub public_key: String,
    pub invite_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub user_id: i64,
    pub username: String,
}

// --- Boards ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub thread_count: i64,
    pub last_post_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BoardListResponse {
    pub boards: Vec<Board>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardInfo {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub description: String,
}

// --- Threads ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: i64,
    pub title: String,
    pub author: String,
    pub created_at: String,
    pub last_post_at: String,
    pub post_count: i64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub latest_post_id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadListResponse {
    pub board: BoardInfo,
    pub threads: Vec<ThreadSummary>,
    pub page: i64,
    pub total_pages: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDetail {
    pub id: i64,
    pub board_id: i64,
    pub board_slug: String,
    pub title: String,
    pub author: String,
    pub created_at: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: i64,
    pub post_number: i64,
    pub author: String,
    pub body: String,
    pub created_at: String,
    #[serde(default)]
    pub edited_at: Option<String>,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub attachments: Vec<AttachmentInfo>,
    #[serde(default)]
    pub parent_post_id: Option<i64>,
    #[serde(default)]
    pub parent_post_number: Option<i64>,
    #[serde(default)]
    pub parent_author: Option<String>,
    #[serde(default)]
    pub reactions: Vec<ReactionCount>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadViewResponse {
    pub thread: ThreadDetail,
    pub posts: Vec<Post>,
    pub page: i64,
    pub total_pages: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateThreadRequest {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateThreadResponse {
    pub thread_id: i64,
    pub post_id: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePostRequest {
    pub body: String,
    #[serde(default)]
    pub parent_post_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePostResponse {
    pub post_id: i64,
    pub post_number: i64,
}

// --- Post editing ---

#[derive(Debug, Serialize, Deserialize)]
pub struct EditPostRequest {
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EditPostResponse {
    pub post_id: i64,
    pub edit_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostEdit {
    pub old_body: String,
    pub edited_at: String,
    #[serde(default)]
    pub edited_by: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostHistoryResponse {
    pub post_id: i64,
    pub current_body: String,
    pub edits: Vec<PostEdit>,
}

// --- Moderation ---

#[derive(Debug, Serialize, Deserialize)]
pub struct ModActionRequest {
    pub action: String, // "pin", "unpin", "lock", "unlock", "delete_post", "restore_post", "ban", "unban", "set_role"
    #[serde(default)]
    pub target_user: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModActionResponse {
    pub success: bool,
    pub message: String,
}

// --- Invites ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteInfo {
    pub code: String,
    pub used_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InviteListResponse {
    pub invites: Vec<InviteInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InviteCreateResponse {
    pub code: String,
}

// --- User ---

#[derive(Debug, Serialize, Deserialize)]
pub struct MeResponse {
    pub user_id: i64,
    pub username: String,
    pub created_at: String,
    pub invited_by: Option<String>,
    pub role: String,
    #[serde(default)]
    pub bio: String,
}

// --- User list / who's online ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub joined_at: String,
    pub last_seen_at: Option<String>,
    pub invited_by: Option<String>,
    pub post_count: i64,
    pub is_online: bool,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub bio: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserListResponse {
    pub users: Vec<UserInfo>,
}

// --- Search ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub kind: String,
    pub thread_id: i64,
    pub post_id: i64,
    pub snippet: String,
    pub thread_title: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub page: i64,
    pub total_pages: i64,
}

// --- Bookmarks ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkInfo {
    pub thread_id: i64,
    pub thread_title: String,
    pub board_slug: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BookmarkListResponse {
    pub bookmarks: Vec<BookmarkInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BookmarkToggleResponse {
    pub bookmarked: bool,
}

// --- Attachments ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInfo {
    pub id: i64,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadAttachmentRequest {
    pub filename: String,
    pub content_type: String,
    pub data_base64: String, // base64-encoded file data
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadAttachmentResponse {
    pub attachment_id: i64,
    pub filename: String,
}

// --- Direct Messages ---

#[derive(Debug, Serialize, Deserialize)]
pub struct SendDmRequest {
    pub recipient: String,
    pub ciphertext: String,
    pub nonce: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendDmResponse {
    pub dm_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmMessage {
    pub id: i64,
    pub sender: String,
    pub ciphertext: String,
    pub nonce: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DmConversationSummary {
    pub username: String,
    pub public_key: String,
    pub last_message_at: String,
    pub message_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DmInboxResponse {
    pub conversations: Vec<DmConversationSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DmConversationResponse {
    pub partner: String,
    pub partner_public_key: String,
    pub messages: Vec<DmMessage>,
    pub page: i64,
    pub total_pages: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserPublicKeyResponse {
    pub public_key: String,
}

// --- Reactions ---

/// Maximum byte length for a reaction emoji string.
pub const MAX_REACTION_LEN: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionCount {
    pub reaction: String,
    pub count: i64,
    #[serde(default)]
    pub reacted_by_me: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReactRequest {
    pub reaction: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReactResponse {
    pub added: bool,
    pub reaction: String,
}

// --- Bio ---

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateBioRequest {
    pub bio: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateBioResponse {
    pub bio: String,
}

// --- Mentions ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionResult {
    pub post_id: i64,
    pub thread_id: i64,
    pub thread_title: String,
    pub author: String,
    pub snippet: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MentionsResponse {
    pub mentions: Vec<MentionResult>,
    pub page: i64,
    pub total_pages: i64,
}

// --- Errors ---

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}
