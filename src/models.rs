use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
    pub source_engine: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
    pub engines: Vec<String>,
    pub score: f64,
}

/// Query parameters extracted from the HTTP request by the Axum handler.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    10
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<AggregatedResult>,
    pub engines_queried: Vec<String>,
    pub engines_failed: Vec<String>,
}
