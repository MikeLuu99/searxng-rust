pub mod duckduckgo;
pub mod brave;

use std::time::Duration;
use reqwest::{Client, header::{self, HeaderMap, HeaderValue}};

// Mimic a real browser as closely as possible to avoid bot-detection rejections.
// These headers are what Chrome 124 sends on a fresh navigation.
pub fn build_http_client() -> anyhow::Result<Client> {
    let mut headers = HeaderMap::new();

    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/124.0.0.0 Safari/537.36",
        ),
    );
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ),
    );
    headers.insert(header::ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate, br"));
    headers.insert("DNT", HeaderValue::from_static("1"));

    // Sec-Fetch-* headers signal a top-level browser navigation (not an XHR/fetch).
    // Some engines reject requests that omit these.
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("document"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("navigate"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("none"));
    headers.insert("Sec-Fetch-User", HeaderValue::from_static("?1"));

    let client = Client::builder()
        .default_headers(headers)
        .cookie_store(true) // DDG sets a consent cookie on first request; persist it
        .gzip(true)
        .brotli(true)
        // Hard socket-level ceiling; per-query timeouts are applied separately
        .timeout(Duration::from_secs(20))
        .build()?;

    Ok(client)
}
