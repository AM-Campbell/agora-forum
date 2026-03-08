use crate::api::ApiClient;
use std::path::Path;

pub async fn upload(
    api: &ApiClient,
    thread_id: i64,
    post_id: i64,
    file_path: &str,
) -> Result<(), String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(format!("File not found: {}", file_path));
    }

    let data = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();

    let content_type = guess_content_type(&filename);

    let size_mb = data.len() as f64 / (1024.0 * 1024.0);
    if data.len() > 5 * 1024 * 1024 {
        return Err(format!(
            "File too large ({:.1} MB). Maximum is 5 MB.",
            size_mb
        ));
    }

    println!(
        "Uploading {} ({:.1} KB, {})...",
        filename,
        data.len() as f64 / 1024.0,
        content_type
    );

    let resp = api
        .upload_attachment(thread_id, post_id, &filename, &content_type, &data)
        .await?;

    println!(
        "Attachment uploaded: {} (id: {})",
        resp.filename, resp.attachment_id
    );
    Ok(())
}

pub async fn download(
    api: &ApiClient,
    attachment_id: i64,
    output: Option<&str>,
) -> Result<(), String> {
    let (data, content_type, filename) = api.download_attachment(attachment_id).await?;

    let out_path = output.unwrap_or(&filename);
    std::fs::write(out_path, &data).map_err(|e| format!("Failed to write file: {}", e))?;

    println!(
        "Downloaded: {} ({:.1} KB)",
        out_path,
        data.len() as f64 / 1024.0
    );

    // Display image inline if terminal supports it
    if crate::cli::image::is_displayable_image(&content_type)
        && crate::cli::image::supports_kitty_graphics()
    {
        if let Err(e) = crate::cli::image::display_image_kitty(&data, &filename) {
            eprintln!("(image display error: {})", e);
        }
    }

    Ok(())
}

fn guess_content_type(filename: &str) -> String {
    let ext = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "js" => "text/javascript",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        _ => "application/octet-stream",
    }
    .to_string()
}
