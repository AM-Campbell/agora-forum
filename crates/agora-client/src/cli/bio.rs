use crate::api::ApiClient;

pub async fn run(api: &ApiClient, text: Option<&str>) -> Result<(), String> {
    match text {
        Some(t) => {
            let resp = api.update_bio(t).await?;
            println!("Bio updated: {}", resp.bio);
        }
        None => {
            let me = api.get_me().await?;
            if me.bio.is_empty() {
                println!("No bio set. Use: agora bio \"your bio text\"");
            } else {
                println!("{}", me.bio);
            }
        }
    }
    Ok(())
}
