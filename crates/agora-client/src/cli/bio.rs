use crate::api::ApiClient;

pub async fn run(api: &ApiClient, text: &str) -> Result<(), String> {
    let resp = api.update_bio(text).await?;
    println!("Bio updated: {}", resp.bio);
    Ok(())
}
