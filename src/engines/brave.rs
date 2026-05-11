use std::time::Duration;

use anyhow::{anyhow, Context};
use reqwest::Client;
use scraper::{Html, Selector};

use crate::models::SearchResult;

const BRAVE_URL: &str = "https://search.brave.com/search";
const TIMEOUT_MS: u64 = 8_000;

pub async fn search(client: &Client, query: &str, max_results: usize) -> anyhow::Result<Vec<SearchResult>> {
    let response = tokio::time::timeout(
        Duration::from_millis(TIMEOUT_MS),
        client
            .get(BRAVE_URL)
            .query(&[("q", query)])
            .send(),
    )
    .await
    .context("Brave request timed out")?
    .context("Brave HTTP request failed")?;

    if !response.status().is_success() {
        return Err(anyhow!("Brave returned status {}", response.status()));
    }

    let body = response.text().await.context("failed to read Brave response body")?;

    parse(&body, max_results)
}

fn parse(html: &str, max_results: usize) -> anyhow::Result<Vec<SearchResult>> {
    let document = Html::parse_document(html);

    // data-type="web" is stable across Brave's Svelte rebuilds; the class names
    // on the same element contain hashed suffixes (e.g. svelte-jmfu5f) that change
    // with every frontend deploy, so we anchor on the data attribute instead.
    let result_sel  = Selector::parse("div[data-type='web']").map_err(|e| anyhow!("selector error: {e:?}"))?;

    // a.l1 is Brave's consistent link class for the primary result anchor.
    // The href on this element is the direct destination URL (no redirect wrapper).
    let link_sel    = Selector::parse("a.l1").map_err(|e| anyhow!("selector error: {e:?}"))?;
    let title_sel   = Selector::parse("div.search-snippet-title").map_err(|e| anyhow!("selector error: {e:?}"))?;
    let snippet_sel = Selector::parse("div.generic-snippet").map_err(|e| anyhow!("selector error: {e:?}"))?;

    let mut results = Vec::new();

    for element in document.select(&result_sel).take(max_results) {
        let Some(link_el) = element.select(&link_sel).next() else { continue };

        let url = link_el.value().attr("href").unwrap_or("").to_string();
        if url.is_empty() || !url.starts_with("http") { continue; }

        let title = element
            .select(&title_sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() { continue; }

        let snippet = element
            .select(&snippet_sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        results.push(SearchResult {
            title,
            url,
            snippet,
            source_engine: "brave".to_string(),
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_extracts_results() {
        let html = r#"
            <div class="snippet" data-type="web">
                <a class="l1" href="https://example.com">
                    <div class="search-snippet-title">Example Site</div>
                </a>
                <div class="generic-snippet">An example website for testing.</div>
            </div>
            <div class="snippet" data-type="web">
                <a class="l1" href="https://rust-lang.org">
                    <div class="search-snippet-title">Rust Language</div>
                </a>
                <div class="generic-snippet">Systems programming language.</div>
            </div>
        "#;

        let results = parse(html, 10).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Example Site");
        assert_eq!(results[0].url, "https://example.com");
        assert_eq!(results[1].url, "https://rust-lang.org");
        assert!(results[0].snippet.is_some());
    }

    #[test]
    fn test_parse_respects_max_results() {
        let result_html = r#"
            <div class="snippet" data-type="web">
                <a class="l1" href="https://example.com">
                    <div class="search-snippet-title">T</div>
                </a>
            </div>
        "#;
        let html = result_html.repeat(5);
        let results = parse(&html, 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_parse_skips_missing_snippet() {
        let html = r#"
            <div class="snippet" data-type="web">
                <a class="l1" href="https://example.com">
                    <div class="search-snippet-title">Title</div>
                </a>
            </div>
        "#;
        let results = parse(html, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].snippet.is_none());
    }

    #[test]
    fn test_parse_skips_non_http_urls() {
        let html = r#"
            <div class="snippet" data-type="web">
                <a class="l1" href="/relative">
                    <div class="search-snippet-title">Relative</div>
                </a>
            </div>
            <div class="snippet" data-type="web">
                <a class="l1" href="https://valid.com">
                    <div class="search-snippet-title">Valid</div>
                </a>
            </div>
        "#;
        let results = parse(html, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://valid.com");
    }

    #[tokio::test]
    #[ignore]
    async fn test_live_search() {
        let client = crate::engines::build_http_client().unwrap();
        let results = search(&client, "rust programming language", 5).await.unwrap();

        println!("Got {} results:", results.len());
        for r in &results {
            println!("  [{}] {}", r.title, r.url);
            if let Some(s) = &r.snippet {
                println!("    snippet: {}", &s[..s.len().min(80)]);
            }
        }

        assert!(!results.is_empty(), "expected at least one result from Brave");
        for r in &results {
            assert!(!r.title.is_empty());
            assert!(r.url.starts_with("http"));
        }
    }
}
