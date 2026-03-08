use crate::api::ApiClient;
use crate::cache;
use crate::editor;

pub async fn run(
    api: &ApiClient,
    db: &cache::Cache,
    thread_id: i64,
    file: Option<&str>,
    reply_context: usize,
    reply_to_post_number: Option<i64>,
) -> Result<(), String> {
    let body = if let Some(path) = file {
        if path == "-" {
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
        // Fetch thread for context
        let context = match api.get_thread(thread_id, 1).await {
            Ok(resp) => {
                cache::cache_posts(db, thread_id, &resp.posts);
                let posts = &resp.posts;
                let recent: Vec<_> = if posts.len() > reply_context {
                    posts[posts.len() - reply_context..].to_vec()
                } else {
                    posts.to_vec()
                };
                editor::build_reply_context(
                    &resp.thread.title,
                    thread_id,
                    &resp.thread.board_slug,
                    &recent,
                )
            }
            Err(_) => {
                // Try cache
                let posts = cache::get_cached_posts(db, thread_id);
                let recent: Vec<_> = if posts.len() > reply_context {
                    posts[posts.len() - reply_context..].to_vec()
                } else {
                    posts.to_vec()
                };
                let title = format!("Thread #{}", thread_id);
                editor::build_reply_context(&title, thread_id, "unknown", &recent)
            }
        };

        match editor::open_editor(&format!("reply_{}", thread_id), &context)? {
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

    // Resolve --to post number to post_id (search all pages)
    let parent_post_id = if let Some(post_num) = reply_to_post_number {
        let resp = api.get_thread(thread_id, 1).await?;
        let mut found_id = resp.posts.iter().find(|p| p.post_number == post_num).map(|p| p.id);
        if found_id.is_none() && resp.total_pages > 1 {
            for page in 2..=resp.total_pages {
                if let Ok(page_resp) = api.get_thread(thread_id, page).await {
                    if let Some(p) = page_resp.posts.iter().find(|p| p.post_number == post_num) {
                        found_id = Some(p.id);
                        break;
                    }
                }
            }
        }
        match found_id {
            Some(id) => Some(id),
            None => return Err(format!("Post #{} not found in thread {}", post_num, thread_id)),
        }
    } else {
        None
    };

    let result = if let Some(pid) = parent_post_id {
        api.create_post_reply(thread_id, &body, pid).await
    } else {
        api.create_post(thread_id, &body).await
    };

    match result {
        Ok(resp) => {
            println!(
                "Reply posted! Post ID: {}, Post #{}",
                resp.post_id, resp.post_number
            );
        }
        Err(e) => {
            let draft_path = editor::draft_path(&format!("reply_{}", thread_id));
            std::fs::write(&draft_path, &body).ok();
            return Err(format!(
                "Failed to post reply: {}\nDraft saved to: {}\nSubmit later with: agora reply {} -f {}",
                e,
                draft_path.display(),
                thread_id,
                draft_path.display()
            ));
        }
    }

    Ok(())
}
