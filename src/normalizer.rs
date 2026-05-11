use url::Url;

// Query parameters added by ad/analytics platforms that don't affect page content.
// Two URLs differing only by these params are the same page for deduplication purposes.
const TRACKING_PARAMS: &[&str] = &[
    "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
    "fbclid", "gclid", "msclkid", "yclid", "ref", "source",
];

/// Normalize a URL to a canonical string used as the deduplication key.
///
/// Two raw URLs referring to the same page should produce the same key.
/// Returns None if the URL cannot be parsed — callers should skip those results.
pub fn normalize(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;

    // Fragments are client-side only — #section-1 and #section-2 are the same page
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

    // Strip trailing slash from non-root paths (/page/ → /page, / stays /)
    let path = url.path().to_string();
    if path.len() > 1 && path.ends_with('/') {
        url.set_path(path.trim_end_matches('/'));
    }

    Some(url.to_string().to_lowercase())
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
}
