//! [`Call`]: one socket request as plain data, and [`raw_call`] for a method
//! that has no typed call.

use std::time::Duration;

use serde_json::Value;

use super::reply::raw;
use crate::herdr_api::error::HerdrError;

pub(super) type Parse<T> = fn(Result<Value, HerdrError>) -> Result<T, HerdrError>;

/// One socket request and how to read its reply.
#[must_use = "a Call does nothing until a client sends it"]
pub struct Call<T> {
    method: &'static str,
    params: Value,
    pub(super) timeout: Option<Duration>,
    parse: Parse<T>,
}

impl<T> Clone for Call<T> {
    fn clone(&self) -> Self {
        Self {
            method: self.method,
            params: self.params.clone(),
            timeout: self.timeout,
            parse: self.parse,
        }
    }
}

impl<T> std::fmt::Debug for Call<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("method", &self.method)
            .field("params", &self.params)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl<T> Call<T> {
    pub(super) fn new(method: &'static str, params: Value, parse: Parse<T>) -> Self {
        Self {
            method,
            params,
            timeout: None,
            parse,
        }
    }

    /// The socket method.
    pub fn method(&self) -> &'static str {
        self.method
    }

    /// The params, as they go on the wire.
    pub fn params(&self) -> &Value {
        &self.params
    }

    /// This call's own timeout, when it needs longer than the client's.
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    /// Read a reply (or the transport's error) as this call's result.
    pub fn parse(&self, reply: Result<Value, HerdrError>) -> Result<T, HerdrError> {
        (self.parse)(reply)
    }
}

/// Any method, params as given (not cleaned), the raw `result`. For what
/// has no typed call yet.
pub fn raw_call(method: &'static str, params: Value) -> Call<Value> {
    Call::new(method, params, raw)
}
