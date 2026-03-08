use crate::api::ApiClient;

pub async fn list(api: &ApiClient) -> Result<(), String> {
    let resp = api.list_bookmarks().await?;

    if resp.bookmarks.is_empty() {
        println!("No bookmarks yet. Use `agora bookmark <thread_id>` to bookmark a thread.");
        return Ok(());
    }

    println!(
        "{:<8} {:<12} {}",
        "Thread", "Board", "Title"
    );
    println!("{}", "─".repeat(60));

    for bm in &resp.bookmarks {
        println!(
            "#{:<7} {:<12} {}",
            bm.thread_id, bm.board_slug, bm.thread_title
        );
    }

    Ok(())
}

pub async fn toggle(api: &ApiClient, thread_id: i64) -> Result<(), String> {
    let resp = api.toggle_bookmark(thread_id).await?;
    if resp.bookmarked {
        println!("Thread #{} bookmarked.", thread_id);
    } else {
        println!("Thread #{} unbookmarked.", thread_id);
    }
    Ok(())
}
