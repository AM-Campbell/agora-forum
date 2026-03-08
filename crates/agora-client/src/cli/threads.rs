use crate::api::ApiClient;
use crate::cache;

pub async fn run(api: &ApiClient, db: &cache::Cache, board_slug: &str) -> Result<(), String> {
    match api.get_threads(board_slug, 1).await {
        Ok(resp) => {
            cache::cache_threads(db, resp.board.id, &resp.threads);
            println!("Board: {} — {}\n", resp.board.name, resp.board.description);
            print_threads(&resp.threads);
            println!("\npage {}/{}", resp.page, resp.total_pages);
        }
        Err(e) => {
            eprintln!("Warning: {}", e);
            eprintln!("Showing cached data.\n");
            // We need board_id from cache
            let boards = cache::get_cached_boards(db);
            let board = boards.iter().find(|b| b.slug == board_slug);
            match board {
                Some(b) => {
                    let threads = cache::get_cached_threads(db, b.id);
                    if threads.is_empty() {
                        return Err("No cached threads available.".to_string());
                    }
                    print_threads(&threads);
                }
                None => return Err("Board not found in cache.".to_string()),
            }
        }
    }
    Ok(())
}

fn print_threads(threads: &[agora_common::ThreadSummary]) {
    println!(
        "{:<6} {:<32} {:<14} {:>5}  {}",
        "ID", "Title", "Author", "Posts", "Latest"
    );
    println!("{}", "-".repeat(78));
    for t in threads {
        let title = if t.title.len() > 30 {
            format!("{}..", &t.title[..28])
        } else {
            t.title.clone()
        };
        let author = if t.author.len() > 12 {
            format!("{}..", &t.author[..10])
        } else {
            t.author.clone()
        };
        println!(
            "{:<6} {:<32} {:<14} {:>5}  {}",
            t.id, title, author, t.post_count, t.last_post_at
        );
    }
}
