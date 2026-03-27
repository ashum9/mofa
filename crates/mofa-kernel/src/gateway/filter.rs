use async_trait::async_trait;
use crate::gateway::{GatewayContext, GatewayRequest, GatewayResponse};
use std::collections::HashMap;
use std::fmt::Debug;
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

