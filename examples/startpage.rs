/// Run with: cargo run --example startpage -- "your query"
use std::sync::Arc;

use metadata_search_engine_rs::engines::{SearchEngine, StartpageEngine, build_http_client};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let query = std::env::args().nth(1).unwrap_or_else(|| "rust programming".to_string());
    let max_results = 5;

    println!("Searching Startpage for: {query:?}\n");

    let client = Arc::new(build_http_client()?);
    let engine = StartpageEngine { client };

    match engine.search(&query, max_results).await {
        Ok(results) => {
            println!("Got {} result(s):\n", results.len());
            for (i, r) in results.iter().enumerate() {
                println!("  #{} {}", i + 1, r.title);
                println!("      {}", r.url);
                if let Some(snippet) = &r.snippet {
                    println!("      {}", &snippet[..snippet.len().min(120)]);
                }
                println!();
            }
        }
        Err(e) => eprintln!("Error: {e}"),
    }

    Ok(())
}
