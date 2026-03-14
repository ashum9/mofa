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

