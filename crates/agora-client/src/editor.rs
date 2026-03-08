use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use crate::config;

/// Open $EDITOR with a temp file and return the user's input.
/// Lines starting with "# " are stripped.
/// Returns None if the content is empty after stripping.
pub fn open_editor(prefix: &str, initial_content: &str) -> Result<Option<String>, String> {
    let drafts_dir = config::drafts_dir();
    std::fs::create_dir_all(&drafts_dir)
        .map_err(|e| format!("Failed to create drafts directory: {}", e))?;

    let timestamp = chrono::Utc::now().timestamp();
    let filename = format!("{}_{}.txt", prefix, timestamp);
    let path = drafts_dir.join(&filename);

    let mut file = std::fs::File::create(&path)
        .map_err(|e| format!("Failed to create draft file: {}", e))?;
    file.write_all(initial_content.as_bytes())
        .map_err(|e| format!("Failed to write draft: {}", e))?;
    drop(file);

    let editor = config::get_editor();
    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .map_err(|e| format!(
            "Failed to launch editor '{}': {}. Set the $EDITOR environment variable or configure 'editor' in ~/.agora/config.toml.",
            editor, e
        ))?;

    if !status.success() {
        return Err(format!("Editor exited with status: {}", status));
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read draft: {}", e))?;

    let body = strip_comments(&content);

    if body.trim().is_empty() {
        std::fs::remove_file(&path).ok();
        return Ok(None);
    }

    Ok(Some(body))
}

/// Get the draft file path for a given prefix (used to keep drafts on failure).
pub fn draft_path(prefix: &str) -> PathBuf {
    let drafts_dir = config::drafts_dir();
    let timestamp = chrono::Utc::now().timestamp();
    drafts_dir.join(format!("{}_{}.txt", prefix, timestamp))
}

/// Build reply context content for the editor.
pub fn build_reply_context(
    thread_title: &str,
    thread_id: i64,
    board_slug: &str,
    recent_posts: &[agora_common::Post],
) -> String {
    let mut content = String::new();
    content.push_str(&format!("# Replying to: {}\n", thread_title));
    content.push_str(&format!("# Thread #{} in {}\n", thread_id, board_slug));
    content.push_str("#\n");
    content.push_str("# --- Recent posts (for context, will not be included) ---\n");
    content.push_str("#\n");

    for post in recent_posts {
        content.push_str(&format!(
            "# [#{}] {} ({}):\n",
            post.post_number, post.author, post.created_at
        ));
        for line in post.body.lines() {
            content.push_str(&format!("# > {}\n", line));
        }
        content.push_str("#\n");
    }

    content.push_str("# --- Write your reply below this line ---\n");
    content.push('\n');
    content
}

/// Build reply-to context for replying to a specific post.
pub fn build_reply_to_context(
    thread_title: &str,
    thread_id: i64,
    board_slug: &str,
    parent_post: &agora_common::Post,
) -> String {
    let mut content = String::new();
    content.push_str(&format!("# Replying to post #{} by {}\n", parent_post.post_number, parent_post.author));
    content.push_str(&format!("# Thread: {} (#{} in {})\n", thread_title, thread_id, board_slug));
    content.push_str("#\n");
    content.push_str("# --- Original post (for context, will not be included) ---\n");
    content.push_str("#\n");
    for line in parent_post.body.lines() {
        content.push_str(&format!("# > {}\n", line));
    }
    content.push_str("#\n");
    content.push_str("# --- Write your reply below this line ---\n");
    content.push('\n');
    content
}

fn strip_comments(content: &str) -> String {
    content
        .lines()
        .filter(|line| !line.starts_with("# ") && *line != "#")
        .collect::<Vec<_>>()
        .join("\n")
}
