/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Config-driven builders for JJ EdenAPI clients.

use std::collections::HashMap;
use std::time::Duration;

use url::Url;

use crate::errors::ConfigError;
use crate::errors::JjRemoteApiError;

/// Top-level builder that instantiates a `JjRemoteApi` impl from configuration.
pub struct Builder {
    repo_name: Option<String>,
    server_url: Option<Url>,
    headers: HashMap<String, String>,
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    max_retry_per_request: usize,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            repo_name: None,
            server_url: None,
            headers: HashMap::new(),
            timeout: Some(Duration::from_secs(300)),
            connect_timeout: Some(Duration::from_secs(30)),
            max_retry_per_request: 3,
        }
    }
}

impl Builder {
    /// Create a Builder with hard-coded defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the repo name (used to build `/{repo}/jjapi/…` paths).
    pub fn repo_name(mut self, name: impl ToString) -> Self {
        self.repo_name = Some(name.to_string());
        self
    }

    /// Set the base server URL.
    pub fn server_url(mut self, url: Url) -> Self {
        self.server_url = Some(url);
        self
    }

    /// Add an extra HTTP header that should be sent on every request.
    pub fn header(mut self, name: impl ToString, value: impl ToString) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Set the overall request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the TCP connect timeout.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Set the maximum number of retries per request.
    pub fn max_retry_per_request(mut self, max: usize) -> Self {
        self.max_retry_per_request = max;
        self
    }

    /// Build a `Client` from explicit configuration.
    pub fn build(self) -> Result<crate::client::Client, JjRemoteApiError> {
        let server_url: Url = self
            .server_url
            .ok_or_else(|| ConfigError::Missing("jjapi.url".to_string()))?;
        let repo_name: String = self
            .repo_name
            .ok_or_else(|| ConfigError::Missing("jjapi.reponame".to_string()))?;

        // Ensure the path ends with '/'; Url::join strips the final component otherwise.
        let mut server_url = server_url;
        if !server_url.path().ends_with('/') {
            server_url.set_path(&format!("{}/", server_url.path()));
        }

        let reqwest_client = reqwest::ClientBuilder::new()
            .http2_prior_knowledge()
            .build()?;

        let config = crate::client::ClientConfig {
            repo_name,
            server_url,
            headers: self.headers,
            timeout: self.timeout,
            max_retry_per_request: self.max_retry_per_request,
        };

        Ok(crate::client::Client::with_config(config, reqwest_client))
    }
}

