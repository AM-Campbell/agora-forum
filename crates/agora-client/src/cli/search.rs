use crate::api::ApiClient;

pub async fn run(api: &ApiClient, query: &str, by: Option<&str>, page: i64) -> Result<(), String> {
    let resp = api.search(query, by, page).await?;

    if resp.results.is_empty() {
        if let Some(user) = by {
            if query.is_empty() {
                println!("No posts found by: {}", user);
            } else {
                println!("No results found for: {} (by {})", query, user);
            }
        } else {
            println!("No results found for: {}", query);
        }
        return Ok(());
    }

    if let Some(user) = by {
        if query.is_empty() {
            println!("Posts by: {}\n", user);
        } else {
            println!("Search results for: {} (by {})\n", query, user);
        }
    } else {
        println!("Search results for: {}\n", query);
    }

    for result in &resp.results {
        let title = result
            .thread_title
            .as_deref()
            .unwrap_or("(unknown thread)");
        let author = result.author.as_deref().unwrap_or("?");
        let kind_label = match result.kind.as_str() {
            "thread" => "Thread",
            "post" => "Post",
            _ => &result.kind,
        };

        println!(
            "  [{}] Thread #{}: {} (by {})",
            kind_label, result.thread_id, title, author
        );
        if !result.snippet.is_empty() {
            println!("    {}", result.snippet);
        }
        println!();
    }

    println!(
        "Page {}/{} — {} result(s)",
        resp.page,
        resp.total_pages,
        resp.results.len()
    );

    Ok(())
}
