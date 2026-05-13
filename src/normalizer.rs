use url::Url;

// Query parameters added by ad/analytics platforms that don't affect page content.
// Two URLs differing only by these params are the same page for deduplication purposes.
const TRACKING_PARAMS: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "fbclid",
    "gclid",
    "msclkid",
    "yclid",
    "ref",
    "source",
];

// Index filenames that are semantically equivalent to the directory path.
// /page/index.html and /page/ resolve to the same content on virtually all servers.
const INDEX_FILES: &[&str] = &["index.html", "index.htm", "index.php"];

/// Normalize a URL to a canonical string used as the deduplication key.
///
/// Two raw URLs referring to the same page should produce the same key.
/// Returns None if the URL cannot be parsed — callers should skip those results.
pub fn normalize(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;

    url.set_fragment(None);

    let clean: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !TRACKING_PARAMS.contains(&k.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if clean.is_empty() {
        url.set_query(None);
    } else {
        let mut sorted = clean;
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let qs = sorted
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        url.set_query(Some(&qs));
    }

    let path = url.path().to_string();
    let path = strip_locale_prefix(&path);
    let path = strip_index_file(path);

    // Strip trailing slash from non-root paths (/page/ → /page, / stays /)
    let path = if path.len() > 1 && path.ends_with('/') {
        path.trim_end_matches('/').to_string()
    } else {
        path.to_string()
    };

    url.set_path(&path);

    Some(url.to_string().to_lowercase())
}

/// Strip a leading locale segment from a URL path.
///
/// Matches 2-letter language codes (e.g. `/en/`) and language-region codes
/// (e.g. `/en-US/`, `/en_US/`) only when followed by another slash, so that
/// short but legitimate path segments like `/go` are left untouched.
fn strip_locale_prefix(path: &str) -> &str {
    let rest = match path.strip_prefix('/') {
        Some(r) => r,
        None => return path,
    };

    // Require a trailing slash after the segment — /en/docs not /en (bare segment)
    let (segment, remainder) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => return path,
    };

    if is_locale_segment(segment) {
        remainder
    } else {
        path
    }
}

/// Returns true if `s` looks like a BCP 47 locale code used as a path prefix.
/// Matches: `en`, `fr`, `en-US`, `en_US`, `zh-CN`, `pt-BR` (2 or 5 chars).
fn is_locale_segment(s: &str) -> bool {
    let b = s.as_bytes();
    match b.len() {
        2 => b[0].is_ascii_alphabetic() && b[1].is_ascii_alphabetic(),
        5 => {
            b[0].is_ascii_alphabetic()
                && b[1].is_ascii_alphabetic()
                && (b[2] == b'-' || b[2] == b'_')
                && b[3].is_ascii_alphabetic()
                && b[4].is_ascii_alphabetic()
        }
        _ => false,
    }
}

/// Strip index filenames so /page/index.html and /page/ produce the same path.
fn strip_index_file(path: &str) -> &str {
    for index in INDEX_FILES {
        if let Some(dir) = path.strip_suffix(index) {
            return dir;
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_removes_tracking_params() {
        let n = normalize("https://example.com/page?utm_source=google&q=rust").unwrap();
        assert!(!n.contains("utm_source"));
        assert!(n.contains("q=rust"));
    }

    #[test]
    fn test_removes_fragment() {
        let a = normalize("https://example.com/page#section").unwrap();
        let b = normalize("https://example.com/page").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_removes_trailing_slash() {
        let a = normalize("https://example.com/page/").unwrap();
        let b = normalize("https://example.com/page").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_root_slash_preserved() {
        let n = normalize("https://example.com/").unwrap();
        assert!(n.ends_with('/') || n == "https://example.com");
    }

    #[test]
    fn test_sorts_query_params() {
        let a = normalize("https://example.com/?z=1&a=2").unwrap();
        let b = normalize("https://example.com/?a=2&z=1").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_lowercases_scheme_and_host() {
        let a = normalize("HTTPS://Example.COM/page").unwrap();
        let b = normalize("https://example.com/page").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_returns_none_for_invalid_url() {
        assert!(normalize("not a url").is_none());
    }

    #[test]
    fn test_strips_locale_language_only() {
        let a = normalize("https://example.com/en/docs").unwrap();
        let b = normalize("https://example.com/docs").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_strips_locale_language_region_hyphen() {
        let a = normalize("https://rust-lang.org/en-US/").unwrap();
        let b = normalize("https://rust-lang.org/").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_strips_locale_language_region_underscore() {
        let a = normalize("https://example.com/en_US/page").unwrap();
        let b = normalize("https://example.com/page").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_does_not_strip_bare_short_segment() {
        // /go with no trailing slash — strip_locale_prefix requires a following /
        // so this is left untouched even though "go" is 2 letters
        let n = normalize("https://example.com/go").unwrap();
        assert!(n.contains("/go"));
    }

    #[test]
    fn test_strips_index_html() {
        let a = normalize("https://example.com/page/index.html").unwrap();
        let b = normalize("https://example.com/page").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_strips_index_htm() {
        let a = normalize("https://example.com/page/index.htm").unwrap();
        let b = normalize("https://example.com/page").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_strips_index_php() {
        let a = normalize("https://example.com/page/index.php").unwrap();
        let b = normalize("https://example.com/page").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_combined_locale_and_index() {
        let a = normalize("https://example.com/en-US/page/index.html").unwrap();
        let b = normalize("https://example.com/page").unwrap();
        assert_eq!(a, b);
    }
}
