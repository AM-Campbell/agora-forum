use crate::api::ApiClient;

pub async fn run(api: &ApiClient, page: i64) -> Result<(), String> {
    let resp = api.get_mentions(page).await?;
    if resp.mentions.is_empty() {
        println!("No mentions found.");
        return Ok(());
    }
    println!("Mentions (page {}/{}):", resp.page, resp.total_pages);
    println!("{}", "=".repeat(60));
    for m in &resp.mentions {
        println!(
            "[Thread #{}] {} — by {} ({})",
            m.thread_id, m.thread_title, m.author, m.created_at
        );
        println!("  {}", m.snippet);
        println!();
    }
    Ok(())
}
