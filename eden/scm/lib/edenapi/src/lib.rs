/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! JJ EdenAPI HTTP client.
//!
//! Product-facing external client for JJ operations through the JJAPI server.
//! Identical in architecture to Sapling's `edenapi` crate.
//!
//! Usage:
//! ```rust,no_run
//! use jj_edenapi::{Builder, JjRemoteApi};
//!
//! async fn demo() {
//!     let client = Builder::new()
//!         .server_url("https://mononoke.example.com".parse().unwrap())
//!         .repo_name("myrepo")
//!         .build()
//!         .unwrap();
//!
//!     let req = jj_edenapi::types::wire::commit::WireReadCommitRequest {
//!         repo: "myrepo".to_string(),
//!         commit_id: vec![1, 2, 3],
//!     };
//!     let resp = client.read_commit(req).await.unwrap();
//!     let commits = resp.flatten().await.unwrap();
//!     println!("commits = {:?}", commits);
//! }
//! ```

mod api;
mod auth;
mod builder;
mod client;
mod errors;
mod response;

// Re-export types crate for convenient fully-qualified access.
pub use jj_edenapi_types as types;

// Re-export the trait and concrete types so callers don't dig deep.
pub use crate::api::JjRemoteApi;
pub use crate::builder::Builder;
pub use crate::client::Client;
pub use crate::client::ClientConfig;
pub use crate::client::paths;
pub use crate::errors::ConfigError;
pub use crate::errors::JjRemoteApiError;
pub use crate::response::Entries;
pub use crate::response::Response;
pub use crate::response::ResponseMeta;

/// Convenience alias for `Result<T, JjRemoteApiError>`.
pub type Result<T> = std::result::Result<T, JjRemoteApiError>;
