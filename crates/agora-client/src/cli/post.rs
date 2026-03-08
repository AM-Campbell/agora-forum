use crate::api::ApiClient;
use crate::editor;

pub async fn run(
    api: &ApiClient,
    board_slug: &str,
    title: &str,
    file: Option<&str>,
) -> Result<(), String> {
    let body = if let Some(path) = file {
        if path == "-" {
            // Read from stdin
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("Failed to read stdin: {}", e))?;
            buf
        } else {
            std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read file '{}': {}", path, e))?
        }
    } else {
        let content = format!(
            "# New thread in: {}\n# Title: {}\n#\n# --- Write your post below this line ---\n\n",
            board_slug, title
        );
        match editor::open_editor(&format!("thread_{}", board_slug), &content)? {
            Some(body) => body,
            None => {
                println!("Empty post, aborting.");
                return Ok(());
            }
        }
    };

    let body = body.trim().to_string();
    if body.is_empty() {
        println!("Empty post, aborting.");
        return Ok(());
    }

    match api.create_thread(board_slug, title, &body).await {
        Ok(resp) => {
            println!(
                "Thread created! ID: {}, Post ID: {}",
                resp.thread_id, resp.post_id
            );
        }
        Err(e) => {
            // Save draft
            let draft_path = editor::draft_path(&format!("thread_{}", board_slug));
            std::fs::write(&draft_path, &body).ok();
            return Err(format!(
                "Failed to create thread: {}\nDraft saved to: {}",
                e,
                draft_path.display()
            ));
        }
    }

    Ok(())
}
