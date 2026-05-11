use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::{
    aggregator::{aggregate, query_all_engines},
    engines::SearchEngine,
    error::AppError,
    models::{SearchQuery, SearchResponse},
};

pub struct AppState {
    pub engines: Vec<Arc<dyn SearchEngine>>,
    pub results_per_engine: usize,
    pub max_results: usize,
}

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    params: Result<Query<SearchQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<(StatusCode, Json<SearchResponse>), AppError> {
    let Query(params) = match params {
        Ok(p) => p,
        Err(_) => return Err(AppError::bad_request("query parameter 'q' is required")),
    };

    let query = params.q.trim().to_string();

    if query.is_empty() {
        return Err(AppError::bad_request("query parameter 'q' cannot be empty"));
    }

    let (successes, failures) =
        query_all_engines(&state.engines, &query, state.results_per_engine).await;

    let engines_queried: Vec<String> = state.engines.iter().map(|e| e.name().to_string()).collect();
    let engines_failed: Vec<String> = failures.iter().map(|(name, _)| name.clone()).collect();

    for (name, err) in &failures {
        tracing::warn!(engine = %name, error = %err, "engine query failed");
    }

    if successes.is_empty() {
        return Err(AppError::service_unavailable("all engines failed to respond"));
    }

    let results = aggregate(successes, state.max_results);

    Ok((
        StatusCode::OK,
        Json(SearchResponse {
            query,
            results,
            engines_queried,
            engines_failed,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engines::BoxFuture, error::EngineError, models::SearchResult, server::build_router};
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    // Minimal engine that returns a fixed set of results without network calls.
    struct MockEngine {
        name: &'static str,
        results: Vec<SearchResult>,
    }

    impl SearchEngine for MockEngine {
        fn name(&self) -> &'static str {
            self.name
        }

        fn search<'a>(
            &'a self,
            _query: &'a str,
            max_results: usize,
        ) -> BoxFuture<'a, Result<Vec<SearchResult>, EngineError>> {
            let results = self.results.iter().take(max_results).cloned().collect();
            Box::pin(async move { Ok(results) })
        }
    }

    fn mock_result(title: &str, url: &str, engine: &str) -> SearchResult {
        SearchResult {
            title: title.to_string(),
            url: url.to_string(),
            snippet: Some(format!("{title} snippet")),
            source_engine: engine.to_string(),
        }
    }

    fn build_test_router(engines: Vec<Arc<dyn SearchEngine>>) -> axum::Router {
        let state = Arc::new(AppState {
            engines,
            results_per_engine: 10,
            max_results: 10,
        });
        build_router(state)
    }

    // Parse response body bytes into a JSON value.
    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_health_returns_ok() {
        let router = build_test_router(vec![]);

        let response = router
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn test_search_missing_query_returns_400() {
        let router = build_test_router(vec![]);

        let response = router
            .oneshot(Request::builder().uri("/search").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_search_empty_query_returns_400() {
        let router = build_test_router(vec![Arc::new(MockEngine {
            name: "mock",
            results: vec![],
        })]);

        let response = router
            .oneshot(Request::builder().uri("/search?q=").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_body(response).await;
        assert!(body["error"].as_str().unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn test_search_all_engines_fail_returns_503() {
        struct FailingEngine;

        impl SearchEngine for FailingEngine {
            fn name(&self) -> &'static str { "failing" }

            fn search<'a>(
                &'a self,
                _query: &'a str,
                _max_results: usize,
            ) -> BoxFuture<'a, Result<Vec<SearchResult>, EngineError>> {
                Box::pin(async { Err(EngineError::Timeout { engine: "failing" }) })
            }
        }

        let router = build_test_router(vec![Arc::new(FailingEngine)]);

        let response = router
            .oneshot(Request::builder().uri("/search?q=rust").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = json_body(response).await;
        assert!(body["error"].as_str().unwrap().contains("all engines failed"));
    }

    #[tokio::test]
    async fn test_search_returns_aggregated_results() {
        let engines: Vec<Arc<dyn SearchEngine>> = vec![
            Arc::new(MockEngine {
                name: "engine_a",
                results: vec![
                    mock_result("Rust Lang", "https://rust-lang.org", "engine_a"),
                    mock_result("Rust Book", "https://doc.rust-lang.org/book", "engine_a"),
                ],
            }),
            Arc::new(MockEngine {
                name: "engine_b",
                results: vec![
                    mock_result("Rust Lang", "https://rust-lang.org", "engine_b"),
                    mock_result("Wikipedia", "https://en.wikipedia.org/wiki/Rust", "engine_b"),
                ],
            }),
        ];

        let router = build_test_router(engines);

        let response = router
            .oneshot(Request::builder().uri("/search?q=rust").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;

        assert_eq!(body["query"], "rust");
        let results = body["results"].as_array().unwrap();
        assert!(!results.is_empty());

        // rust-lang.org should rank first — returned by both engines at rank 1
        assert_eq!(results[0]["url"], "https://rust-lang.org");
        assert_eq!(results[0]["engines"].as_array().unwrap().len(), 2);

        assert_eq!(body["engines_queried"].as_array().unwrap().len(), 2);
        assert_eq!(body["engines_failed"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_search_partial_engine_failure_still_returns_results() {
        struct FailingEngine;

        impl SearchEngine for FailingEngine {
            fn name(&self) -> &'static str { "failing" }
            fn search<'a>(
                &'a self,
                _query: &'a str,
                _max_results: usize,
            ) -> BoxFuture<'a, Result<Vec<SearchResult>, EngineError>> {
                Box::pin(async { Err(EngineError::Timeout { engine: "failing" }) })
            }
        }

        let engines: Vec<Arc<dyn SearchEngine>> = vec![
            Arc::new(MockEngine {
                name: "working",
                results: vec![mock_result("Rust", "https://rust-lang.org", "working")],
            }),
            Arc::new(FailingEngine),
        ];

        let router = build_test_router(engines);

        let response = router
            .oneshot(Request::builder().uri("/search?q=rust").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;

        let results = body["results"].as_array().unwrap();
        assert!(!results.is_empty());

        let failed = body["engines_failed"].as_array().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0], "failing");
    }
}
