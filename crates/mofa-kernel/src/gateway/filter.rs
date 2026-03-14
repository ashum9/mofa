use async_trait::async_trait;
use crate::gateway::{GatewayRequest, GatewayResponse};
use std::collections::HashMap;
use std::fmt::Debug;

/// Represents the error states unique to Gateway filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayError {
    Unauthorized(String),
    RateLimited(String),
    ValidationFailed(String),
    Internal(String),
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            Self::RateLimited(msg) => write!(f, "Rate Limited: {}", msg),
            Self::ValidationFailed(msg) => write!(f, "Validation Failed: {}", msg),
            Self::Internal(msg) => write!(f, "Internal Error: {}", msg),
        }
    }
}

impl std::error::Error for GatewayError {}

/// The mutable context passed through the filter chain.
#[derive(Debug, Clone)]
pub struct FilterContext {
    pub request: GatewayRequest,
    pub metadata: HashMap<String, String>,
    pub agent_id: Option<String>,
}

impl FilterContext {
    pub fn new(request: GatewayRequest) -> Self {
        Self {
            request,
            metadata: HashMap::new(),
            agent_id: None,
        }
    }
}

/// The result returned by a single filter.
#[derive(Debug, Clone)]
pub enum FilterResult {
    /// Proceed to the next filter in the chain.
    Pass(FilterContext),
    /// Immediately reject the request with a response.
    Reject(GatewayResponse),
    /// Modify and immediately return a successful response without calling further filters.
    Return(GatewayResponse),
}

/// The primary trait that all interceptor patterns (e.g., auth, rate-limit, telemetry) implement.
#[async_trait]
pub trait GatewayFilter: Send + Sync + Debug {
    async fn execute(&self, ctx: FilterContext) -> Result<FilterResult, GatewayError>;
}

/// The executor engine that iterates through registered filters.
#[derive(Default)]
pub struct FilterChain {
    filters: Vec<Box<dyn GatewayFilter>>,
}

impl FilterChain {
    pub fn new() -> Self {
        Self { filters: Vec::new() }
    }

    pub fn add_filter(&mut self, filter: Box<dyn GatewayFilter>) {
        self.filters.push(filter);
    }

    /// Executes the full sequence of filters on a given context.
    /// Returns either the completely processed `FilterContext` or an immediate `GatewayResponse`
    /// if a filter decided to short-circuit the chain.
    pub async fn run(&self, ctx: FilterContext) -> Result<Result<FilterContext, GatewayResponse>, GatewayError> {
        let mut current_ctx = ctx;

        for filter in &self.filters {
            match filter.execute(current_ctx).await? {
                FilterResult::Pass(next_ctx) => {
                    current_ctx = next_ctx;
                }
                FilterResult::Reject(response) => {
                    return Ok(Err(response)); // Execution halts, return the rejection
                }
                FilterResult::Return(response) => {
                    return Ok(Err(response)); // Execution halts, return early response
                }
            }
        }

        Ok(Ok(current_ctx)) // All filters passed
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
            ctx.metadata.insert("mock_accept".to_string(), "true".to_string());
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

    #[tokio::test]
    async fn test_filter_chain() {
        let mut chain = FilterChain::new();
        chain.add_filter(Box::new(MockAcceptFilter));
        chain.add_filter(Box::new(MockRejectFilter));

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
            Ok(_) => panic!("Expected filter chain to short-circuit and reject!"),
            Err(response) => {
                assert_eq!(response.status, 403);
                assert_eq!(response.body, b"Forbidden by MockRejectFilter".to_vec());
            }
        }
    }
    
    #[tokio::test]
    async fn test_filter_chain_pass() {
        let mut chain = FilterChain::new();
        chain.add_filter(Box::new(MockAcceptFilter));

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
            Ok(processed_ctx) => {
                assert_eq!(processed_ctx.metadata.get("mock_accept"), Some(&"true".to_string()));
            }
            Err(_) => panic!("Expected filter chain to pass!"),
        }
    }
}



