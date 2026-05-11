# metadata-search-engine-rs

A SearXNG-style metadata search engine written in Rust. Fans out queries to multiple search engines concurrently, scrapes their HTML results, deduplicates by normalized URL, and ranks using Reciprocal Rank Fusion (RRF).

## How it works

1. A search request arrives at `GET /search?q=<query>`
2. The query is sent concurrently to DuckDuckGo, Brave, and Startpage via `reqwest`
3. Each engine parses the HTML response with `scraper` (CSS selectors over Mozilla's html5ever)
4. Results are deduplicated by normalized URL (tracking params stripped, locale prefixes removed, query params sorted)
5. Duplicate URLs are merged and scored with RRF (`score = Σ 1/(60 + rank)` across engines) — pages returned by multiple engines rank higher
6. The top results are returned as JSON

## Requirements

- Rust 1.75+ (uses `impl Trait` in return position for the `SearchEngine` trait)
- Cargo

## Installation

```bash
git clone <repo-url>
cd metadata-search-engine-rs
cargo build --release
```

## Running

```bash
cargo run --release
```

The server starts on port 3000 by default. Override with environment variables:

| Variable | Default | Description |
|---|---|---|
| `PORT` | `3000` | TCP port to listen on |
| `ENGINE_TIMEOUT_MS` | `8000` | Per-engine request timeout (ms) |
| `RESULTS_PER_ENGINE` | `10` | Results fetched from each engine |
| `MAX_RESULTS` | `10` | Aggregated results returned to caller |

Example with custom config:

```bash
PORT=8080 MAX_RESULTS=20 cargo run --release
```

Enable debug logging:

```bash
RUST_LOG=debug cargo run
```

## API

### `GET /health`

```bash
curl http://localhost:3000/health
```

```json
{"status": "ok"}
```

### `GET /search?q=<query>`

```bash
curl "http://localhost:3000/search?q=rust"
```

```json
{
  "query": "rust",
  "results": [
    {
      "title": "Rust Programming Language",
      "url": "https://rust-lang.org/",
      "snippet": "A language empowering everyone to build reliable and efficient software.",
      "engines": ["duckduckgo", "brave", "startpage"],
      "score": 0.049
    }
  ],
  "engines_queried": ["duckduckgo", "brave", "startpage"],
  "engines_failed": []
}
```

**Error responses:**

| Case | Status | Body |
|---|---|---|
| Missing `q` | 400 | `{"error": "query parameter 'q' is required"}` |
| Empty `q` | 400 | `{"error": "query parameter 'q' cannot be empty"}` |
| All engines fail | 503 | `{"error": "all engines failed to respond"}` |

## Running tests

```bash
# All unit tests
cargo test

# Specific module
cargo test normalizer
cargo test aggregator
cargo test engines::duckduckgo
cargo test engines::brave
cargo test engines::startpage
cargo test server::handlers

# Live tests (hit real search engines — requires internet)
cargo test -- --ignored test_live
```

Live tests are marked `#[ignore]` so they don't run in CI by default. Run them manually to verify HTML selectors still work against the real sites.

## Project structure

```
src/
├── main.rs           # Entry point — wires engines, state, and router
├── lib.rs            # Module declarations
├── config.rs         # AppConfig read from environment variables
├── error.rs          # EngineError (typed) and AppError (HTTP responses)
├── models.rs         # SearchResult, AggregatedResult, SearchQuery, SearchResponse
├── normalizer.rs     # URL normalization for deduplication
├── aggregator.rs     # Fan-out query and RRF-based aggregation
├── engines/
│   ├── mod.rs        # SearchEngine trait, shared HTTP client
│   ├── duckduckgo.rs # html.duckduckgo.com/html/ scraper
│   ├── brave.rs      # search.brave.com scraper
│   └── startpage.rs  # startpage.com scraper
└── server/
    ├── mod.rs        # Router setup with CORS and tracing middleware
    └── handlers.rs   # GET /health and GET /search handlers + tests
```

## Adding a new engine

1. Create `src/engines/<name>.rs`
2. Define a struct holding `Arc<reqwest::Client>`
3. Implement the `SearchEngine` trait:

```rust
impl SearchEngine for MyEngine {
    fn name(&self) -> &'static str { "myengine" }

    fn search<'a>(
        &'a self,
        query: &'a str,
        max_results: usize,
    ) -> BoxFuture<'a, Result<Vec<SearchResult>, EngineError>> {
        Box::pin(async move {
            // fetch HTML, parse with scraper, return Vec<SearchResult>
        })
    }
}
```

4. Add it to `engines/mod.rs` and wire it in `main.rs`
