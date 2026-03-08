use crate::api::ApiClient;

pub async fn run(api: &ApiClient, thread_id: i64, post_id: i64, reaction: &str) -> Result<(), String> {
    let resp = api.react_post(thread_id, post_id, reaction).await?;
    if resp.added {
        println!("Reaction '{}' added to post {}.", resp.reaction, post_id);
    } else {
        println!("Reaction '{}' removed from post {}.", resp.reaction, post_id);
    }
    Ok(())
}
