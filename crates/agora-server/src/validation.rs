use agora_common::*;
use base64::Engine;

/// Validate a username: 3-20 chars, alphanumeric + underscore, no leading underscore.
/// Input should already be trimmed and lowercased.
pub fn validate_username(s: &str) -> Result<(), &'static str> {
    if s.len() < MIN_USERNAME_LEN || s.len() > MAX_USERNAME_LEN {
        return Err("Username must be 3-20 characters");
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Username may only contain alphanumeric characters and underscores");
    }
    if s.starts_with('_') {
        return Err("Username may not start with an underscore");
    }
    Ok(())
}

/// Validate a base64-encoded ed25519 public key (must decode to exactly 32 bytes).
pub fn validate_public_key_b64(s: &str) -> Result<(), &'static str> {
    match base64::engine::general_purpose::STANDARD.decode(s) {
        Ok(bytes) if bytes.len() == 32 => Ok(()),
        Ok(_) => Err("Invalid public key length"),
        Err(_) => Err("Invalid public key encoding"),
    }
}

/// Validate a post body: trimmed input must be 1..=MAX_BODY_LEN bytes.
pub fn validate_post_body(s: &str) -> Result<(), &'static str> {
    if s.is_empty() || s.len() > MAX_BODY_LEN {
        return Err("Invalid post body length");
    }
    Ok(())
}

/// Validate a thread title: trimmed input must be 1..=MAX_TITLE_LEN bytes.
pub fn validate_thread_title(s: &str) -> Result<(), &'static str> {
    if s.is_empty() || s.len() > MAX_TITLE_LEN {
        return Err("Invalid thread title length");
    }
    Ok(())
}

/// Sanitize a filename by stripping control characters and path separators.
pub fn sanitize_filename(s: &str) -> String {
    s.trim()
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .collect()
}

/// Escape a raw search query for FTS5: split on whitespace, remove quotes,
/// wrap each word in double-quotes to prevent FTS syntax injection.
pub fn escape_fts_query(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let safe: String = word.chars().filter(|c| *c != '"').collect();
            format!("\"{}\"", safe)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Escape special characters for a SQL LIKE pattern.
pub fn escape_sql_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Verify that the leading bytes of `data` match the expected magic bytes
/// for the given content type. Returns true if they match (or if the content
/// type is not one we check), false if they definitely don't match.
pub fn verify_content_type_magic(data: &[u8], content_type: &str) -> bool {
    match content_type {
        "image/png" => data.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
        "image/jpeg" => data.starts_with(&[0xFF, 0xD8, 0xFF]),
        "image/gif" => data.starts_with(b"GIF8"),
        "image/webp" => data.len() >= 12 && &data[8..12] == b"WEBP",
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_username ---

    #[test]
    fn username_valid() {
        assert!(validate_username("alice").is_ok());
        assert!(validate_username("bob_42").is_ok());
        assert!(validate_username("abc").is_ok()); // minimum length
        assert!(validate_username("a".repeat(20).as_str()).is_ok()); // max length
    }

    #[test]
    fn username_too_short() {
        assert!(validate_username("ab").is_err());
        assert!(validate_username("").is_err());
    }

    #[test]
    fn username_too_long() {
        assert!(validate_username(&"a".repeat(21)).is_err());
    }

    #[test]
    fn username_special_chars() {
        assert!(validate_username("alice!").is_err());
        assert!(validate_username("bob smith").is_err());
        assert!(validate_username("test@user").is_err());
        assert!(validate_username("a-b").is_err());
    }

    #[test]
    fn username_leading_underscore() {
        assert!(validate_username("_alice").is_err());
        assert!(validate_username("___").is_err());
    }

    // --- validate_public_key_b64 ---

    #[test]
    fn public_key_valid() {
        // 32 zero bytes in base64
        let key = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        assert!(validate_public_key_b64(&key).is_ok());
    }

    #[test]
    fn public_key_wrong_length() {
        let key = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        assert_eq!(validate_public_key_b64(&key), Err("Invalid public key length"));
    }

    #[test]
    fn public_key_invalid_base64() {
        assert_eq!(validate_public_key_b64("not-valid-base64!!!"), Err("Invalid public key encoding"));
    }

    // --- validate_post_body ---

    #[test]
    fn post_body_valid() {
        assert!(validate_post_body("Hello world").is_ok());
        assert!(validate_post_body("x").is_ok());
    }

    #[test]
    fn post_body_empty() {
        assert!(validate_post_body("").is_err());
    }

    #[test]
    fn post_body_too_long() {
        let long = "x".repeat(MAX_BODY_LEN + 1);
        assert!(validate_post_body(&long).is_err());
    }

    #[test]
    fn post_body_at_max() {
        let exact = "x".repeat(MAX_BODY_LEN);
        assert!(validate_post_body(&exact).is_ok());
    }

    // --- validate_thread_title ---

    #[test]
    fn thread_title_valid() {
        assert!(validate_thread_title("My Thread").is_ok());
        assert!(validate_thread_title("x").is_ok());
    }

    #[test]
    fn thread_title_empty() {
        assert!(validate_thread_title("").is_err());
    }

    #[test]
    fn thread_title_too_long() {
        let long = "x".repeat(MAX_TITLE_LEN + 1);
        assert!(validate_thread_title(&long).is_err());
    }

    // --- sanitize_filename ---

    #[test]
    fn sanitize_filename_normal() {
        assert_eq!(sanitize_filename("photo.png"), "photo.png");
    }

    #[test]
    fn sanitize_filename_path_traversal() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "....etcpasswd");
        assert_eq!(sanitize_filename("..\\..\\secret"), "....secret");
    }

    #[test]
    fn sanitize_filename_control_chars() {
        assert_eq!(sanitize_filename("file\x00name\x01.txt"), "filename.txt");
    }

    #[test]
    fn sanitize_filename_with_spaces_trimmed() {
        assert_eq!(sanitize_filename("  photo.png  "), "photo.png");
    }

    #[test]
    fn sanitize_filename_null_bytes() {
        assert_eq!(sanitize_filename("file\x00.txt"), "file.txt");
        assert_eq!(sanitize_filename("\x00\x00\x00"), "");
    }

    #[test]
    fn sanitize_filename_unicode() {
        // Unicode names should pass through (only control chars and path seps stripped)
        assert_eq!(sanitize_filename("café.txt"), "café.txt");
        assert_eq!(sanitize_filename("日本語.png"), "日本語.png");
    }

    #[test]
    fn sanitize_filename_very_long() {
        let long_name = "a".repeat(500) + ".txt";
        let result = sanitize_filename(&long_name);
        // sanitize_filename doesn't truncate — length check is done by the route handler
        assert_eq!(result.len(), 504);
    }

    #[test]
    fn sanitize_filename_empty_after_sanitize() {
        // All control chars → empty string
        assert_eq!(sanitize_filename("\x01\x02\x03"), "");
        // Only slashes → empty string
        assert_eq!(sanitize_filename("///"), "");
    }

    #[test]
    fn sanitize_filename_dots_only() {
        // Dots are allowed — the route handler checks the result
        assert_eq!(sanitize_filename(".."), "..");
        assert_eq!(sanitize_filename("..."), "...");
    }

    // --- escape_fts_query ---

    #[test]
    fn fts_query_normal_words() {
        assert_eq!(escape_fts_query("hello world"), "\"hello\" \"world\"");
    }

    #[test]
    fn fts_query_words_with_quotes() {
        assert_eq!(escape_fts_query("he\"llo"), "\"hello\"");
    }

    #[test]
    fn fts_query_empty() {
        assert_eq!(escape_fts_query(""), "");
        assert_eq!(escape_fts_query("   "), "");
    }

    #[test]
    fn fts_query_single_word() {
        assert_eq!(escape_fts_query("rust"), "\"rust\"");
    }

    // --- escape_sql_like ---

    #[test]
    fn sql_like_normal_text() {
        assert_eq!(escape_sql_like("alice"), "alice");
    }

    #[test]
    fn sql_like_with_percent() {
        assert_eq!(escape_sql_like("100%"), "100\\%");
    }

    #[test]
    fn sql_like_with_underscore() {
        assert_eq!(escape_sql_like("a_b"), "a\\_b");
    }

    #[test]
    fn sql_like_with_backslash() {
        assert_eq!(escape_sql_like("a\\b"), "a\\\\b");
    }

    #[test]
    fn sql_like_all_special() {
        assert_eq!(escape_sql_like("a%b_c\\d"), "a\\%b\\_c\\\\d");
    }

    // --- verify_content_type_magic ---

    #[test]
    fn magic_valid_png() {
        let data = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(verify_content_type_magic(&data, "image/png"));
    }

    #[test]
    fn magic_valid_jpeg() {
        let data = [0xFF, 0xD8, 0xFF, 0xE0];
        assert!(verify_content_type_magic(&data, "image/jpeg"));
    }

    #[test]
    fn magic_valid_gif() {
        assert!(verify_content_type_magic(b"GIF89a...", "image/gif"));
        assert!(verify_content_type_magic(b"GIF87a...", "image/gif"));
    }

    #[test]
    fn magic_valid_webp() {
        let mut data = vec![0u8; 12];
        data[8..12].copy_from_slice(b"WEBP");
        assert!(verify_content_type_magic(&data, "image/webp"));
    }

    #[test]
    fn magic_mismatched_type() {
        // JPEG data claimed as PNG
        let data = [0xFF, 0xD8, 0xFF, 0xE0];
        assert!(!verify_content_type_magic(&data, "image/png"));
    }

    #[test]
    fn magic_too_short_for_webp() {
        let data = [0u8; 4];
        assert!(!verify_content_type_magic(&data, "image/webp"));
    }

    #[test]
    fn magic_unknown_type_always_passes() {
        assert!(verify_content_type_magic(&[0x00], "application/pdf"));
        assert!(verify_content_type_magic(&[], "text/plain"));
    }
}
