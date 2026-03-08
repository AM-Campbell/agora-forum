use crate::api::ApiClient;

pub async fn generate(api: &ApiClient) -> Result<(), String> {
    let resp = api.create_invite().await?;
    println!("{}", resp.code);
    Ok(())
}

pub async fn list(api: &ApiClient) -> Result<(), String> {
    let resp = api.get_invites().await?;

    println!("{:<18} {:<20} {}", "Code", "Status", "Created");
    println!("{}", "-".repeat(56));
    for inv in &resp.invites {
        let status = match &inv.used_by {
            Some(user) => format!("used by: {}", user),
            None => "unused".to_string(),
        };
        println!("{:<18} {:<20} {}", inv.code, status, inv.created_at);
    }

    let unused = resp.invites.iter().filter(|i| i.used_by.is_none()).count();
    println!("\n({} invites remaining)", 5 - unused);

    Ok(())
}
