use crate::api::ApiClient;
use crate::cache;
use crate::cli::image;

pub async fn run(api: &ApiClient, db: &cache::Cache, thread_id: i64, page: Option<i64>) -> Result<(), String> {
    let can_show_images = image::supports_kitty_graphics();
    let start_page = page.unwrap_or(1);

    match api.get_thread(thread_id, start_page).await {
        Ok(resp) => {
            cache::cache_posts(db, thread_id, &resp.posts);
            if let Some(last_post) = resp.posts.last() {
                cache::mark_thread_read(db, thread_id, last_post.id);
            }
            print_thread(&resp.thread, &resp.posts);

            // Try to display images inline for posts with image attachments
            if can_show_images {
                display_images(api, &resp.posts).await;
            }

            // Fetch remaining pages (only when no specific page requested)
            if page.is_none() && resp.total_pages > 1 {
                for p in 2..=resp.total_pages {
                    if let Ok(page_resp) = api.get_thread(thread_id, p).await {
                        cache::cache_posts(db, thread_id, &page_resp.posts);
                        if let Some(last_post) = page_resp.posts.last() {
                            cache::mark_thread_read(db, thread_id, last_post.id);
                        }
                        for post in &page_resp.posts {
                            print_post(post);
                        }
                        if can_show_images {
                            display_images(api, &page_resp.posts).await;
                        }
                    }
                }
            }

            if resp.total_pages > 1 {
                println!("page {}/{}", resp.page, resp.total_pages);
            }
        }
        Err(e) => {
            eprintln!("Warning: {}", e);
            eprintln!("Showing cached data.\n");
            let posts = cache::get_cached_posts(db, thread_id);
            if posts.is_empty() {
                return Err("No cached posts available.".to_string());
            }
            println!("Thread #{}\n", thread_id);
            for post in &posts {
                print_post(post);
            }
        }
    }
    Ok(())
}

fn print_thread(thread: &agora_common::ThreadDetail, posts: &[agora_common::Post]) {
    let mut flags = Vec::new();
    if thread.pinned {
        flags.push("[PINNED]");
    }
    if thread.locked {
        flags.push("[LOCKED]");
    }
    let flag_str = if flags.is_empty() {
        String::new()
    } else {
        format!(" {}", flags.join(" "))
    };

    println!("{}{}", thread.title, flag_str);
    println!(
        "Thread #{} in {} by {}",
        thread.id, thread.board_slug, thread.author
    );
    println!("{}", "=".repeat(60));
    println!();

    for post in posts {
        print_post(post);
    }
}

fn print_post(post: &agora_common::Post) {
    let edited = if post.edited_at.is_some() {
        " (edited)"
    } else {
        ""
    };

    if post.is_deleted {
        println!(
            "[#{}] {} ({}){} [DELETED]",
            post.post_number, post.author, post.created_at, edited
        );
        println!("{}", "-".repeat(60));
        println!("[This post has been deleted by a moderator]");
    } else {
        println!(
            "[#{}] {} ({}){}",
            post.post_number, post.author, post.created_at, edited
        );
        if let Some(parent_num) = post.parent_post_number {
            let parent_author = post.parent_author.as_deref().unwrap_or("?");
            println!("  re: #{} ({})", parent_num, parent_author);
        }
        println!("{}", "-".repeat(60));
        println!("{}", post.body);
    }

    // Show reactions (skip for deleted posts)
    if !post.is_deleted && !post.reactions.is_empty() {
        let reaction_str: Vec<String> = post
            .reactions
            .iter()
            .map(|rc| {
                let emoji = match rc.reaction.as_str() {
                    "thumbsup" => "+1",
                    "check" => "ok",
                    "heart" => "<3",
                    "think" => "hmm",
                    "laugh" => "ha",
                    other => other,
                };
                let you = if rc.reacted_by_me { "*" } else { "" };
                format!("{} {}{}", emoji, rc.count, you)
            })
            .collect();
        println!("  Reactions: {}", reaction_str.join("  "));
    }

    // Show attachments
    if !post.attachments.is_empty() {
        println!();
        let can_show_images = image::supports_kitty_graphics();
        for att in &post.attachments {
            let is_image = image::is_displayable_image(&att.content_type);
            let icon = if is_image { "🖼" } else { "📎" };
            print!(
                "  {} {} ({}, {:.1} KB)",
                icon,
                att.filename,
                att.content_type,
                att.size_bytes as f64 / 1024.0,
            );
            if is_image && !can_show_images {
                println!(" — agora download {} (image; use kitty/ghostty/wezterm for inline display)", att.id);
            } else {
                println!(" — agora download {}", att.id);
            }
        }
    }

    println!();
}

/// Download and display images inline for posts that have image attachments.
async fn display_images(api: &ApiClient, posts: &[agora_common::Post]) {
    for post in posts {
        for att in &post.attachments {
            if image::is_displayable_image(&att.content_type) {
                match api.download_attachment(att.id).await {
                    Ok((data, _ct, _fname)) => {
                        if let Err(e) = image::display_image_kitty(&data, &att.filename) {
                            eprintln!("  (image display error: {})", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("  (failed to download image {}: {})", att.filename, e);
                    }
                }
            }
        }
    }
}
