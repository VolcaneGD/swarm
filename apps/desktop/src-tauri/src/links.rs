use tauri::Url;

/// Validate and canonicalize a URL before it is handed to the system browser.
/// Only absolute HTTP(S) URLs with a host and no user information are accepted.
pub fn normalize_external_url(input: &str) -> Result<String, String> {
    if input.is_empty()
        || input.chars().any(|character| {
            character.is_whitespace() || character.is_control() || character == '\\'
        })
        || contains_forbidden_encoded_byte(input)
    {
        return Err("URL contains forbidden whitespace or characters".into());
    }

    let separator = input
        .find("://")
        .ok_or_else(|| "URL must be absolute and include an authority".to_owned())?;
    let raw_scheme = &input[..separator];
    if !raw_scheme.eq_ignore_ascii_case("http") && !raw_scheme.eq_ignore_ascii_case("https") {
        return Err("Only HTTP and HTTPS URLs are allowed".into());
    }

    let authority_start = separator + 3;
    let authority_end = input[authority_start..]
        .find(['/', '?', '#'])
        .map(|offset| authority_start + offset)
        .unwrap_or(input.len());
    let authority = &input[authority_start..authority_end];
    if authority.is_empty() {
        return Err("URL must include a host".into());
    }
    // Checking the raw authority also rejects empty user-info forms such as
    // `https://@host` that a parsed URL may normalize to an empty username.
    if authority.contains('@') {
        return Err("URL user information is not allowed".into());
    }

    let url = Url::parse(input).map_err(|_| "URL is invalid".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only HTTP and HTTPS URLs are allowed".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("URL must include a host".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URL user information is not allowed".into());
    }
    Ok(url.into())
}

fn contains_forbidden_encoded_byte(input: &str) -> bool {
    input.as_bytes().windows(3).any(|triplet| {
        if triplet[0] != b'%' {
            return false;
        }
        let (Some(high), Some(low)) = (
            (triplet[1] as char).to_digit(16),
            (triplet[2] as char).to_digit(16),
        ) else {
            return false;
        };
        let decoded = ((high << 4) | low) as u8;
        decoded <= 0x1f || decoded == 0x7f || decoded == b'\\'
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_external_url;

    #[test]
    fn accepts_normal_http_urls_and_normalizes_unicode_hosts() {
        assert_eq!(
            normalize_external_url("https://example.com/path?q=two#section").unwrap(),
            "https://example.com/path?q=two#section"
        );
        assert_eq!(
            normalize_external_url("http://[2001:db8::1]:8080/a").unwrap(),
            "http://[2001:db8::1]:8080/a"
        );
        let unicode = normalize_external_url("https://例え.テスト/道?q=日本語#章").unwrap();
        assert!(unicode.starts_with("https://xn--r8jz45g.xn--zckzah/"));
        assert!(unicode.contains("?q=%E6%97%A5%E6%9C%AC%E8%AA%9E#%E7%AB%A0"));
    }

    #[test]
    fn rejects_non_http_relative_and_malformed_authorities() {
        for input in [
            "",
            "example.com/path",
            "/relative/path",
            "//example.com/path",
            "mailto:user@example.com",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https:example.com/path",
            "http:///missing-host",
            "https:////example.com/",
            "https://",
            "https://example.com:bad/",
        ] {
            assert!(normalize_external_url(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn rejects_user_information_including_empty_credentials() {
        for input in [
            "https://user@example.com/",
            "https://user:password@example.com/",
            "https://:password@example.com/",
            "https://@example.com/",
            "https://:@example.com/",
        ] {
            assert!(normalize_external_url(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn rejects_whitespace_controls_and_backslashes_anywhere() {
        for input in [
            " https://example.com/",
            "https://example.com/path with-space",
            "https://example.com/path\tvalue",
            "https://example.com/path\nvalue",
            "https://example.com/path\u{0085}value",
            "https://example.com\\@evil.example/",
            "https://example.com/%00",
            "https://example.com/%1f",
            "https://example.com/%7F",
            "https://example.com/%5c",
            "https://example.com/%5C",
        ] {
            assert!(normalize_external_url(input).is_err(), "accepted {input:?}");
        }
    }
}
