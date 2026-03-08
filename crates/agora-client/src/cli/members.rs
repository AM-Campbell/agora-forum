use crate::api::ApiClient;

pub async fn members(api: &ApiClient) -> Result<(), String> {
    let resp = api.get_users().await?;

    println!(
        "{:<16} {:<8} {:<20} {:<16} {:<6} {:<8} {}",
        "Username", "Role", "Joined", "Invited by", "Posts", "Status", "Bio"
    );
    println!("{}", "-".repeat(92));

    for user in &resp.users {
        let invited = user.invited_by.as_deref().unwrap_or("-");
        let status = if user.is_online { "online" } else { "offline" };
        let bio = if user.bio.is_empty() { "-" } else { &user.bio };
        println!(
            "{:<16} {:<8} {:<20} {:<16} {:<6} {:<8} {}",
            user.username, user.role, user.joined_at, invited, user.post_count, status, bio
        );
    }

    let online = resp.users.iter().filter(|u| u.is_online).count();
    println!("\n{} members total, {} online", resp.users.len(), online);

    Ok(())
}

pub async fn who(api: &ApiClient) -> Result<(), String> {
    let resp = api.get_users().await?;

    let online: Vec<_> = resp.users.iter().filter(|u| u.is_online).collect();

    if online.is_empty() {
        println!("No users currently online.");
        return Ok(());
    }

    println!("{:<16} {:<6}", "Username", "Posts");
    println!("{}", "-".repeat(24));

    for user in &online {
        println!("{:<16} {:<6}", user.username, user.post_count);
    }

    println!("\n{} user(s) online", online.len());

    Ok(())
}
