/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::time::Duration;

use http::status::StatusCode;
use thiserror::Error;

/// Error type for the JJ remote API client.
#[derive(Debug, Error)]
pub enum JjRemoteApiError {
    #[error("failed to serialize request: {0}")]
    RequestSerializationFailed(#[source] serde_cbor::Error),
    #[error("failed to parse response: {0}")]
    ParseResponse(String),
    #[error(transparent)]
    BadConfig(#[from] ConfigError),
    #[error("HTTP request failed: {0}")]
    Http(reqwest::Error),
    #[error("server responded {status} for {url}: {message}")]
    HttpError {
        status: StatusCode,
        message: String,
        url: String,
    },
    #[error(transparent)]
    InvalidUrl(#[from] url::ParseError),
    #[error("expected response, but none returned by the server")]
    NoResponse,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Config parsing/validation error.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required config item: {0}")]
    Missing(String),
    #[error("invalid config item: '{0}' ({1})")]
    Invalid(String, #[source] anyhow::Error),
}

impl PartialEq for JjRemoteApiError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::HttpError {
                    status: s1,
                    message: m1,
                    url: u1,
                },
                Self::HttpError {
                    status: s2,
                    message: m2,
                    url: u2,
                },
            ) => s1 == s2 && m1 == m2 && u1 == u2,
            (Self::NoResponse, Self::NoResponse) => true,
            (Self::BadConfig(a), Self::BadConfig(b)) => a.to_string() == b.to_string(),
            _ => false,
        }
    }
}

impl JjRemoteApiError {
    /// Whether this error is retryable (transient).
    pub fn is_retryable(&self) -> bool {
        use JjRemoteApiError::*;
        match self {
            Http(e) => {
                e.is_timeout()
                    || e.is_connect()
                    || e.status()
                        .map_or(false, |s| {
                            s.is_server_error()
                                || s == StatusCode::REQUEST_TIMEOUT
                                || s == StatusCode::TOO_MANY_REQUESTS
                        })
            }
            HttpError { status, .. } => {
                status.is_server_error()
                    || *status == StatusCode::REQUEST_TIMEOUT
                    || *status == StatusCode::TOO_MANY_REQUESTS
            }
            _ => false,
        }
    }

    /// Exponential backoff with jitter for retries.
    pub fn retry_after(&self, attempt: usize, max: usize) -> Option<Duration> {
        if self.is_retryable() && attempt < max {
            let base = if matches!(self, JjRemoteApiError::HttpError { status, .. } if *status == StatusCode::TOO_MANY_REQUESTS)
            {
                2u64.pow(std::cmp::min(attempt as u32, 3))
            } else {
                attempt as u64 + 1
            };
            Some(Duration::from_secs(base).saturating_mul(std::cmp::min(attempt as u32 + 1, 5)))
        } else {
            None
        }
    }
}

impl From<reqwest::Error> for JjRemoteApiError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}
