use base64::Engine;
use std::io::{self, Write};

/// Check if the terminal likely supports the kitty graphics protocol.
/// We check TERM_PROGRAM for known supporting terminals.
pub fn supports_kitty_graphics() -> bool {
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        let t = term.to_lowercase();
        return t.contains("kitty") || t.contains("ghostty") || t.contains("wezterm");
    }
    // Also check TERM for kitty-specific
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("kitty") || term.contains("xterm-kitty") {
            return true;
        }
    }
    false
}

/// Display an image inline using the kitty graphics protocol.
/// The image data should be raw PNG/JPEG/GIF bytes.
pub fn display_image_kitty(data: &[u8], filename: &str) -> io::Result<()> {
    let engine = base64::engine::general_purpose::STANDARD;
    let b64 = engine.encode(data);

    let stdout = io::stdout();
    let mut out = stdout.lock();

    // Kitty graphics protocol:
    // ESC_]Gf=100,a=T,m=1;base64data ESC\
    // For large images, we chunk the base64 data
    //
    // f=100 means auto-detect format
    // a=T means transmit and display
    // m=0 means this is the last (or only) chunk
    // m=1 means more chunks follow

    let chunk_size = 4096;
    let chunks: Vec<&[u8]> = b64.as_bytes().chunks(chunk_size).collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        let m = if is_last { 0 } else { 1 };

        if i == 0 {
            // First chunk includes the control data
            write!(out, "\x1b_Gf=100,a=T,m={};", m)?;
        } else {
            write!(out, "\x1b_Gm={};", m)?;
        }
        out.write_all(chunk)?;
        write!(out, "\x1b\\")?;
    }

    writeln!(out)?;
    writeln!(out, "  {}", filename)?;
    out.flush()?;

    Ok(())
}

/// Returns true if the content type is a displayable image.
pub fn is_displayable_image(content_type: &str) -> bool {
    matches!(
        content_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displayable_image_types() {
        assert!(is_displayable_image("image/png"));
        assert!(is_displayable_image("image/jpeg"));
        assert!(is_displayable_image("image/gif"));
        assert!(is_displayable_image("image/webp"));
    }

    #[test]
    fn non_displayable_types() {
        assert!(!is_displayable_image("image/svg+xml"));
        assert!(!is_displayable_image("application/pdf"));
        assert!(!is_displayable_image("text/plain"));
        assert!(!is_displayable_image("application/octet-stream"));
        assert!(!is_displayable_image(""));
    }

    #[test]
    fn displayable_is_case_sensitive() {
        // matches! is exact — uppercase should not match
        assert!(!is_displayable_image("IMAGE/PNG"));
        assert!(!is_displayable_image("Image/Jpeg"));
    }
}
