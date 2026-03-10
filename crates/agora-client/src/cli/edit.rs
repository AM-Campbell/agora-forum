use crate::api::ApiClient;
use crate::editor;

pub async fn run(
    api: &ApiClient,
    thread_id: i64,
    post_id: i64,
    file: Option<&str>,
) -> Result<(), String> {
    // Get the current post body
    let history = api.post_history(thread_id, post_id).await?;

    let body = match file {
        Some("-") => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("Failed to read stdin: {}", e))?;
            let trimmed = buf.trim().to_string();
            if trimmed.is_empty() {
                println!("Empty body, aborting edit.");
                return Ok(());
            }
            trimmed
        }
        Some(path) => {
            let content =
                std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
            let trimmed = content.trim().to_string();
            if trimmed.is_empty() {
                println!("Empty body, aborting edit.");
                return Ok(());
            }
            trimmed
        }
        None => {
            // Open editor with current body
            let content = format!(
                "# Editing post {} in thread {}\n# Current body is shown below. Edit and save to update.\n# Lines starting with # are comments and will be stripped.\n#\n{}\n\n{}",
                post_id, thread_id, crate::editor::EDITOR_HELP, history.current_body
            );
            match editor::open_editor(&format!("edit_{}_{}", thread_id, post_id), &content)? {
                Some(body) => body.trim().to_string(),
                None => {
                    println!("Empty body, aborting edit.");
                    return Ok(());
                }
            }
        }
    };

    let resp = api.edit_post(thread_id, post_id, &body).await?;
    println!("Post {} edited (edit #{}).", resp.post_id, resp.edit_count);
    Ok(())
}

pub async fn history(
    api: &ApiClient,
    thread_id: i64,
    post_id: i64,
) -> Result<(), String> {
    let resp = api.post_history(thread_id, post_id).await?;

    if resp.edits.is_empty() {
        println!("Post {} has no edit history.", post_id);
        println!("\nCurrent body:\n{}", resp.current_body);
        return Ok(());
    }

    println!("Edit history for post {} ({} edits):\n", post_id, resp.edits.len());

    for (i, edit) in resp.edits.iter().enumerate() {
        let by = edit.edited_by.as_deref().map(|u| format!(" by {}", u)).unwrap_or_default();
        println!("── Version {} (before edit at {}{}) ──", i + 1, edit.edited_at, by);
        println!("{}\n", edit.old_body);
    }

    println!("── Current version ──");
    println!("{}", resp.current_body);

    Ok(())
}
