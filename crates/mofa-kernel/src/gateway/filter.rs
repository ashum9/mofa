use async_trait::async_trait;
use crate::gateway::{GatewayContext, GatewayRequest, GatewayResponse};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use thiserror::Error;

/// Represents the error states unique to Gateway filtering.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum GatewayError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    #[error("Rate Limited: {0}")]
    RateLimited(String),
    #[error("Validation Failed: {0}")]
    ValidationFailed(String),
    #[error("Internal Error: {0}")]
    Internal(String),
}

///
/// This is an alias of [`GatewayContext`], which is the canonical
/// per-request mutable context type for gateway filters.
pub use crate::gateway::GatewayContext as FilterContext;

/// The result returned by a single filter.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum FilterResult {
    /// Proceed to the next filter in the chain.
    Pass(FilterContext),
    /// Immediately reject the request with a response.
    Reject(GatewayResponse),
    /// Modify and immediately return a successful response without calling further filters.
    Return(GatewayResponse),
}

/// The overall outcome of running a filter chain.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ChainOutcome {
    /// All filters passed and produced a final `FilterContext`.
    Completed(FilterContext),
    /// The chain was short-circuited by a filter, which returned a `FilterResult`.
    ///
    /// This allows callers to distinguish between `Reject` and `Return` outcomes.
    ShortCircuit(FilterResult),
}

/// The primary trait that all interceptor patterns (e.g., auth, rate-limit, telemetry) implement.
#[async_trait]
pub trait GatewayFilter: Send + Sync + Debug {
    async fn execute(&self, ctx: FilterContext) -> Result<FilterResult, GatewayError>;
}

/// The executor engine that iterates through registered filters.
#[derive(Default)]
pub struct FilterChain {
    filters: Vec<Arc<dyn GatewayFilter>>,
}

impl FilterChain {
    pub fn new() -> Self {
        Self { filters: Vec::new() }
    }

    pub fn add_filter(&mut self, filter: Arc<dyn GatewayFilter>) {
        self.filters.push(filter);
    }

    /// Executes the full sequence of filters on a given context.
    ///
    /// On success, returns a [`ChainOutcome`] indicating whether the chain completed
    /// with a final [`FilterContext`] or was short-circuited by a filter that returned
    /// a [`FilterResult::Reject`] or [`FilterResult::Return`].
    pub async fn run(&self, ctx: FilterContext) -> Result<ChainOutcome, GatewayError> {
        let mut current_ctx = ctx;

        for filter in &self.filters {
            match filter.execute(current_ctx).await? {
                FilterResult::Pass(next_ctx) => {
                    current_ctx = next_ctx;
                }
                short_circuit @ FilterResult::Reject(_) | short_circuit @ FilterResult::Return(_) => {
                    // Execution halts; propagate the exact short-circuit outcome.
                    return Ok(ChainOutcome::ShortCircuit(short_circuit));
                }
            }
        }

        Ok(ChainOutcome::Completed(current_ctx)) // All filters passed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockAcceptFilter;

    #[async_trait]
    impl GatewayFilter for MockAcceptFilter {
        async fn execute(&self, mut ctx: FilterContext) -> Result<FilterResult, GatewayError> {
            ctx.set_attr("mock_accept", &true);
            Ok(FilterResult::Pass(ctx))
        }
    }

    #[derive(Debug)]
    struct MockRejectFilter;

    #[async_trait]
    impl GatewayFilter for MockRejectFilter {
        async fn execute(&self, _ctx: FilterContext) -> Result<FilterResult, GatewayError> {
            Ok(FilterResult::Reject(GatewayResponse {
                status: 403,
                body: "Forbidden by MockRejectFilter".to_string().into_bytes(),
                headers: HashMap::new(),
                backend_id: "mock".to_string(),
                latency_ms: 0,
            }))
        }
    }

    #[derive(Debug)]
    struct MockReturnFilter;

    #[async_trait]
    impl GatewayFilter for MockReturnFilter {
        async fn execute(&self, _ctx: FilterContext) -> Result<FilterResult, GatewayError> {
            Ok(FilterResult::Return(GatewayResponse {
                status: 200,
                body: b"Mock return early".to_vec(),
                headers: HashMap::new(),
                backend_id: "mock_return".to_string(),
                latency_ms: 0,
            }))
        }
    }

    #[derive(Debug)]
    struct MockErrorFilter;

    #[async_trait]
    impl GatewayFilter for MockErrorFilter {
        async fn execute(&self, _ctx: FilterContext) -> Result<FilterResult, GatewayError> {
            Err(GatewayError::Internal("Mock error filter simulated an internal error".to_string()))
        }
    }

    #[tokio::test]
    async fn test_filter_chain() {
        let mut chain = FilterChain::new();
        chain.add_filter(Arc::new(MockAcceptFilter));
        chain.add_filter(Arc::new(MockRejectFilter));

        let ctx = FilterContext::new(GatewayRequest {
            id: "req_123".to_string(),
            path: "/test".to_string(),
            method: crate::gateway::HttpMethod::Get,
            headers: HashMap::new(),
            body: vec![],
            metadata: HashMap::new(),
        });

        let result = chain.run(ctx).await.unwrap();

        match result {
            ChainOutcome::ShortCircuit(FilterResult::Reject(response)) => {
                assert_eq!(response.status, 403);
                assert_eq!(response.body, b"Forbidden by MockRejectFilter".to_vec());
            }
            _ => panic!("Expected filter chain to short-circuit and reject!"),
        }
    }
    
    #[tokio::test]
    async fn test_filter_chain_pass() {
        let mut chain = FilterChain::new();
        chain.add_filter(Arc::new(MockAcceptFilter));

        let ctx = FilterContext::new(GatewayRequest {
            id: "req_success".to_string(),
            path: "/success".to_string(),
            method: crate::gateway::HttpMethod::Get,
            headers: HashMap::new(),
            body: vec![],
            metadata: HashMap::new(),
        });

        let result = chain.run(ctx).await.unwrap();

        match result {
            ChainOutcome::Completed(processed_ctx) => {
                assert_eq!(processed_ctx.get_attr::<bool>("mock_accept"), Some(true));
            }
            _ => panic!("Expected filter chain to pass!"),
        }
    }

    #[tokio::test]
    async fn test_filter_chain_return() {
        let mut chain = FilterChain::new();
        chain.add_filter(Arc::new(MockAcceptFilter));
        chain.add_filter(Arc::new(MockReturnFilter));

        let ctx = FilterContext::new(GatewayRequest {
            id: "req_return".to_string(),
            path: "/return".to_string(),
            method: crate::gateway::HttpMethod::Get,
            headers: HashMap::new(),
            body: vec![],
            metadata: HashMap::new(),
        });

        let result = chain.run(ctx).await.unwrap();

        match result {
            ChainOutcome::ShortCircuit(FilterResult::Return(response)) => {
                assert_eq!(response.status, 200);
                assert_eq!(response.body, b"Mock return early".to_vec());
            }
            _ => panic!("Expected filter chain to short-circuit and return!"),
        }
    }

    #[tokio::test]
    async fn test_filter_chain_error() {
        let mut chain = FilterChain::new();
        chain.add_filter(Arc::new(MockAcceptFilter));
        chain.add_filter(Arc::new(MockErrorFilter));

        let ctx = FilterContext::new(GatewayRequest {
            id: "req_error".to_string(),
            path: "/error".to_string(),
            method: crate::gateway::HttpMethod::Get,
            headers: HashMap::new(),
            body: vec![],
            metadata: HashMap::new(),
        });

        let result = chain.run(ctx).await;

        match result {
            Err(GatewayError::Internal(msg)) => {
                assert_eq!(msg, "Mock error filter simulated an internal error");
            }
            _ => panic!("Expected filter chain to propagate error!"),
        }
    }
}



