use crate::api::ApiClient;
use crate::cache;

pub async fn run(api: &ApiClient, db: &cache::Cache) -> Result<(), String> {
    match api.get_boards().await {
        Ok(resp) => {
            cache::cache_boards(db, &resp.boards);
            print_boards(&resp.boards);
        }
        Err(e) => {
            eprintln!("Warning: {}", e);
            eprintln!("Showing cached data.\n");
            let boards = cache::get_cached_boards(db);
            if boards.is_empty() {
                return Err("No cached boards available.".to_string());
            }
            print_boards(&boards);
        }
    }
    Ok(())
}

fn print_boards(boards: &[agora_common::Board]) {
    println!(
        "{:<4} {:<24} {:>8}  {}",
        "#", "Board", "Threads", "Latest"
    );
    println!("{}", "-".repeat(60));
    for (i, b) in boards.iter().enumerate() {
        let latest = b
            .last_post_at
            .as_deref()
            .unwrap_or("-");
        println!(
            "{:<4} {:<24} {:>8}  {}",
            i + 1,
            b.name,
            b.thread_count,
            latest
        );
    }
}
