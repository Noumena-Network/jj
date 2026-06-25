/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! HTTP client implementation of `JjRemoteApi`.
//!
//! Pattern: POST CBOR-encoded request bodies, decode CBOR response streams.
//! Retries with exponential backoff on transient errors.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clientinfo::CLIENT_INFO_HEADER;
use clientinfo::ClientEntryPoint;
use clientinfo::ClientInfo;
use futures::channel::mpsc::unbounded;
use jj_edenapi_types::ToWire;
use reqwest::Url;

use crate::api::JjRemoteApi;
use crate::auth;
use crate::errors::JjRemoteApiError;
use crate::response::Entries;
use crate::response::Response;
use crate::response::ResponseMeta;

/// Configuration for the HTTP client.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub repo_name: String,
    pub server_url: Url,
    pub headers: std::collections::HashMap<String, String>,
    pub timeout: Option<Duration>,
    pub max_retry_per_request: usize,
}

/// End-point path constants (relative to `/{repo}/jjapi/`).
pub mod paths {
    pub const COMMIT_READ: &str = "commit/read";
    pub const COMMIT_WRITE: &str = "commit/write";
    pub const TREE_READ: &str = "tree/read";
    pub const TREE_WRITE: &str = "tree/write";
    pub const FILE_READ: &str = "file/read";
    pub const FILE_WRITE: &str = "file/write";
    pub const SYMLINK_READ: &str = "symlink/read";
    pub const SYMLINK_WRITE: &str = "symlink/write";
    pub const BOOKMARK_RESOLVE: &str = "bookmark/resolve";
    pub const BOOKMARK_LIST: &str = "bookmark/list";
    pub const BOOKMARK_CREATE: &str = "bookmark/create";
    pub const BOOKMARK_MOVE: &str = "bookmark/move";
    pub const BOOKMARK_DELETE: &str = "bookmark/delete";
    pub const OPSTORE_READ_VIEW: &str = "opstore/view/read";
    pub const OPSTORE_WRITE_VIEW: &str = "opstore/view/write";
    pub const OPSTORE_READ_OP: &str = "opstore/operation/read";
    pub const OPSTORE_WRITE_OP: &str = "opstore/operation/write";
    pub const OPSTORE_READ_OPHEADS: &str = "opstore/opheads/read";
    pub const OPSTORE_CAS_OPHEADS: &str = "opstore/opheads/cas";
    pub const WORKSPACE_GET: &str = "workspace/get";
    pub const WORKSPACE_PUT: &str = "workspace/put";
    pub const WORKSPACE_LIST: &str = "workspace/list";
    pub const WORKSPACE_DELETE: &str = "workspace/delete";
    pub const HEALTH_CHECK: &str = "health_check";
}

#[derive(Clone, Debug)]
pub struct Client {
    inner: Arc<ClientInner>,
}

#[derive(Debug)]
struct ClientInner {
    config: ClientConfig,
    client: reqwest::Client,
}

impl Client {
    pub(crate) fn with_config(config: ClientConfig, client: reqwest::Client) -> Self {
        Self {
            inner: Arc::new(ClientInner { config, client }),
        }
    }

    pub fn config(&self) -> &ClientConfig {
        &self.inner.config
    }

    /// Append `repo/{base}` onto the server's base URL.
    pub(super) fn build_url(&self, path: &str) -> Result<Url, JjRemoteApiError> {
        let encoded_repo =
            url::form_urlencoded::byte_serialize(self.config().repo_name.as_bytes())
                .collect::<String>();
        self.config()
            .server_url
            .join(&format!("{}/", encoded_repo))?
            .join(&format!("jjapi/{}", path.trim_start_matches('/')))
            .map_err(Into::into)
    }

    fn client_info_header() -> Result<String, JjRemoteApiError> {
        ClientInfo::default_with_entry_point(ClientEntryPoint::SaplingRemoteApi)
            .to_json()
            .map_err(Into::into)
    }

    /// Execute a request, retrying on transient failures, and return a streaming response.
    async fn request_stream<Req, Resp>(
        &self,
        path: &str,
        payload: Req,
    ) -> Result<Response<Resp>, JjRemoteApiError>
    where
        Req: ToWire + serde::Serialize + Send + Sync + 'static,
        Resp: serde::de::DeserializeOwned + Send + 'static,
    {
        let url = self.build_url(path)?;
        let cbor_body = serde_cbor::to_vec(&payload)
            .map_err(JjRemoteApiError::RequestSerializationFailed)?;
        let max_retries = self.config().max_retry_per_request;

        for attempt in 0..=max_retries {
            let mut rb = self.inner.client.post(url.clone());
            for (k, v) in &self.config().headers {
                rb = rb.header(k.as_str(), v.as_str());
            }
            for (k, v) in auth::authentication_headers() {
                rb = rb.header(k.as_str(), v.as_str());
            }
            rb = rb
                .header(CLIENT_INFO_HEADER, Self::client_info_header()?)
                .header("Content-Type", "application/cbor")
                .body(cbor_body.clone());

            match rb.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let body = response
                            .bytes()
                            .await
                            .map_err(JjRemoteApiError::Http)?;
                        let decoded = serde_cbor::from_slice::<Resp>(&body)
                            .map_err(|e| {
                                JjRemoteApiError::ParseResponse(format!("{}", e))
                            })?;
                        let (tx, rx) = unbounded::<Result<Resp, JjRemoteApiError>>();
                        let _ = tx.unbounded_send(Ok(decoded));
                        drop(tx);
                        return Ok(Response {
                            entries: Entries::new(rx),
                        });
                    }

                    let msg = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "unknown".to_string());

                    if status.is_client_error() && status != reqwest::StatusCode::REQUEST_TIMEOUT {
                        return Err(JjRemoteApiError::HttpError {
                            status,
                            message: msg,
                            url: url.to_string(),
                        });
                    }

                    let err = JjRemoteApiError::HttpError {
                        status,
                        message: msg,
                        url: url.to_string(),
                    };
                    if let Some(delay) = err.retry_after(attempt, max_retries) {
                        tracing::warn!(
                            "request to {} returned {}, retry {} of {} in {:?}",
                            url,
                            status,
                            attempt + 1,
                            max_retries,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    } else {
                        return Err(err);
                    }
                }
                Err(e) => {
                    let err = JjRemoteApiError::Http(e);
                    if let Some(delay) = err.retry_after(attempt, max_retries) {
                        tracing::warn!(
                            "request to {} failed, retry {} of {} in {:?}",
                            url,
                            attempt + 1,
                            max_retries,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    } else {
                        return Err(err);
                    }
                }
            }
        }

        Err(JjRemoteApiError::HttpError {
            status: reqwest::StatusCode::GATEWAY_TIMEOUT,
            message: "max retries exceeded".to_string(),
            url: url.to_string(),
        })
    }
}

// ----------------------------------------------------------------------
// JjRemoteApi trait implementation
// ----------------------------------------------------------------------

#[async_trait]
impl JjRemoteApi for Client {
    fn url(&self) -> Option<String> {
        Some(self.config().server_url.to_string())
    }

    async fn health(&self) -> Result<ResponseMeta, JjRemoteApiError> {
        let url = self.config().server_url.join("health_check").map_err(JjRemoteApiError::from)?;
        let mut rb = self.inner.client.get(url.clone());
        for (k, v) in &self.config().headers {
            rb = rb.header(k.as_str(), v.as_str());
        }
        for (k, v) in auth::authentication_headers() {
            rb = rb.header(k.as_str(), v.as_str());
        }
        rb = rb.header(CLIENT_INFO_HEADER, Self::client_info_header()?);

        let response = rb.send().await.map_err(JjRemoteApiError::Http)?;
        let status = response.status();
        if !status.is_success() {
            return Err(JjRemoteApiError::HttpError {
                status,
                message: response.text().await.unwrap_or_else(|_| "unknown".to_string()),
                url: url.to_string(),
            });
        }
        // Health-check body is plain string ("I_AM_ALIVE"); we just confirm success.
        let _body = response
            .text()
            .await
            .unwrap_or_else(|_| "unknown".to_string());
        Ok(ResponseMeta { server_timestamp: 0 })
    }

    // -- commit --
    async fn read_commit(
        &self,
        request: jj_edenapi_types::wire::commit::WireReadCommitRequest,
    ) -> Result<Response<jj_edenapi_types::wire::commit::WireReadCommitResponse>, JjRemoteApiError> {
        self.request_stream(paths::COMMIT_READ, request).await
    }

    async fn write_commit(
        &self,
        request: jj_edenapi_types::wire::commit::WireWriteCommitRequest,
    ) -> Result<Response<jj_edenapi_types::wire::commit::WireWriteCommitResponse>, JjRemoteApiError> {
        self.request_stream(paths::COMMIT_WRITE, request).await
    }

    // -- tree --
    async fn read_tree(
        &self,
        request: jj_edenapi_types::wire::tree::WireReadTreeRequest,
    ) -> Result<Response<jj_edenapi_types::wire::tree::WireReadTreeResponse>, JjRemoteApiError> {
        self.request_stream(paths::TREE_READ, request).await
    }

    async fn write_tree(
        &self,
        request: jj_edenapi_types::wire::tree::WireWriteTreeRequest,
    ) -> Result<Response<jj_edenapi_types::wire::tree::WireWriteTreeResponse>, JjRemoteApiError> {
        self.request_stream(paths::TREE_WRITE, request).await
    }

    // -- file --
    async fn read_file(
        &self,
        request: jj_edenapi_types::wire::file::WireReadFileRequest,
    ) -> Result<Response<jj_edenapi_types::wire::file::WireReadFileResponse>, JjRemoteApiError> {
        self.request_stream(paths::FILE_READ, request).await
    }

    async fn write_file(
        &self,
        request: jj_edenapi_types::wire::file::WireWriteFileRequest,
    ) -> Result<Response<jj_edenapi_types::wire::file::WireWriteFileResponse>, JjRemoteApiError> {
        self.request_stream(paths::FILE_WRITE, request).await
    }

    async fn read_symlink(
        &self,
        request: jj_edenapi_types::wire::file::WireReadSymlinkRequest,
    ) -> Result<Response<jj_edenapi_types::wire::file::WireReadSymlinkResponse>, JjRemoteApiError> {
        self.request_stream(paths::SYMLINK_READ, request).await
    }

    async fn write_symlink(
        &self,
        request: jj_edenapi_types::wire::file::WireWriteSymlinkRequest,
    ) -> Result<Response<jj_edenapi_types::wire::file::WireWriteSymlinkResponse>, JjRemoteApiError> {
        self.request_stream(paths::SYMLINK_WRITE, request).await
    }

    // -- bookmark --
    async fn resolve_bookmark(
        &self,
        request: jj_edenapi_types::wire::bookmark::WireResolveBookmarkRequest,
    ) -> Result<Response<jj_edenapi_types::wire::bookmark::WireResolveBookmarkResponse>, JjRemoteApiError> {
        self.request_stream(paths::BOOKMARK_RESOLVE, request).await
    }

    async fn list_bookmarks(
        &self,
        request: jj_edenapi_types::wire::bookmark::WireListBookmarksRequest,
    ) -> Result<Response<jj_edenapi_types::wire::bookmark::WireListBookmarksResponse>, JjRemoteApiError> {
        self.request_stream(paths::BOOKMARK_LIST, request).await
    }

    async fn create_bookmark(
        &self,
        request: jj_edenapi_types::wire::bookmark::WireCreateBookmarkRequest,
    ) -> Result<Response<jj_edenapi_types::wire::bookmark::WireCreateBookmarkResponse>, JjRemoteApiError> {
        self.request_stream(paths::BOOKMARK_CREATE, request).await
    }

    async fn move_bookmark(
        &self,
        request: jj_edenapi_types::wire::bookmark::WireMoveBookmarkRequest,
    ) -> Result<Response<jj_edenapi_types::wire::bookmark::WireMoveBookmarkResponse>, JjRemoteApiError> {
        self.request_stream(paths::BOOKMARK_MOVE, request).await
    }

    async fn delete_bookmark(
        &self,
        request: jj_edenapi_types::wire::bookmark::WireDeleteBookmarkRequest,
    ) -> Result<Response<jj_edenapi_types::wire::bookmark::WireDeleteBookmarkResponse>, JjRemoteApiError> {
        self.request_stream(paths::BOOKMARK_DELETE, request).await
    }

    // -- opstore --
    async fn read_view(
        &self,
        request: jj_edenapi_types::wire::opstore::WireReadViewRequest,
    ) -> Result<Response<jj_edenapi_types::wire::opstore::WireReadViewResponse>, JjRemoteApiError> {
        self.request_stream(paths::OPSTORE_READ_VIEW, request).await
    }

    async fn write_view(
        &self,
        request: jj_edenapi_types::wire::opstore::WireWriteViewRequest,
    ) -> Result<Response<jj_edenapi_types::wire::opstore::WireWriteViewResponse>, JjRemoteApiError> {
        self.request_stream(paths::OPSTORE_WRITE_VIEW, request).await
    }

    async fn read_operation(
        &self,
        request: jj_edenapi_types::wire::opstore::WireReadOperationRequest,
    ) -> Result<Response<jj_edenapi_types::wire::opstore::WireReadOperationResponse>, JjRemoteApiError> {
        self.request_stream(paths::OPSTORE_READ_OP, request).await
    }

    async fn write_operation(
        &self,
        request: jj_edenapi_types::wire::opstore::WireWriteOperationRequest,
    ) -> Result<Response<jj_edenapi_types::wire::opstore::WireWriteOperationResponse>, JjRemoteApiError> {
        self.request_stream(paths::OPSTORE_WRITE_OP, request).await
    }

    async fn read_op_heads(
        &self,
        request: jj_edenapi_types::wire::opstore::WireReadOpHeadsRequest,
    ) -> Result<Response<jj_edenapi_types::wire::opstore::WireReadOpHeadsResponse>, JjRemoteApiError> {
        self.request_stream(paths::OPSTORE_READ_OPHEADS, request).await
    }

    async fn compare_and_swap_op_heads(
        &self,
        request: jj_edenapi_types::wire::opstore::WireCompareAndSwapOpHeadsRequest,
    ) -> Result<Response<jj_edenapi_types::wire::opstore::WireCompareAndSwapOpHeadsResponse>, JjRemoteApiError> {
        self.request_stream(paths::OPSTORE_CAS_OPHEADS, request).await
    }

    // -- workspace --
    async fn get_workspace(
        &self,
        request: jj_edenapi_types::wire::workspace::WireGetWorkspaceRequest,
    ) -> Result<Response<jj_edenapi_types::wire::workspace::WireGetWorkspaceResponse>, JjRemoteApiError> {
        self.request_stream(paths::WORKSPACE_GET, request).await
    }

    async fn put_workspace(
        &self,
        request: jj_edenapi_types::wire::workspace::WirePutWorkspaceRequest,
    ) -> Result<Response<jj_edenapi_types::wire::workspace::WirePutWorkspaceResponse>, JjRemoteApiError> {
        self.request_stream(paths::WORKSPACE_PUT, request).await
    }

    async fn list_workspaces(
        &self,
        request: jj_edenapi_types::wire::workspace::WireListWorkspacesRequest,
    ) -> Result<Response<jj_edenapi_types::wire::workspace::WireListWorkspacesResponse>, JjRemoteApiError> {
        self.request_stream(paths::WORKSPACE_LIST, request).await
    }

    async fn delete_workspace(
        &self,
        request: jj_edenapi_types::wire::workspace::WireDeleteWorkspaceRequest,
    ) -> Result<Response<jj_edenapi_types::wire::workspace::WireDeleteWorkspaceResponse>, JjRemoteApiError> {
        self.request_stream(paths::WORKSPACE_DELETE, request).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use http::StatusCode;

    use crate::builder::Builder;
    use crate::errors::ConfigError;
    use crate::errors::JjRemoteApiError;

    fn mock_error(status: StatusCode) -> JjRemoteApiError {
        JjRemoteApiError::HttpError {
            status,
            message: "mock".to_string(),
            url: "https://example.com/repo/jjapi/commit/read".to_string(),
        }
    }

    #[test]
    fn test_is_retryable_5xx() {
        let err = mock_error(StatusCode::INTERNAL_SERVER_ERROR);
        assert!(err.is_retryable());
    }

    #[test]
    fn test_is_retryable_429() {
        let err = mock_error(StatusCode::TOO_MANY_REQUESTS);
        assert!(err.is_retryable());
    }

    #[test]
    fn test_is_not_retryable_4xx() {
        let err = mock_error(StatusCode::BAD_REQUEST);
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_retry_after_exponential() {
        let err = mock_error(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.retry_after(0, 3), Some(Duration::from_secs(1)));
        assert_eq!(err.retry_after(1, 3), Some(Duration::from_secs(4)));
        assert_eq!(err.retry_after(2, 3), Some(Duration::from_secs(9)));
    }

    #[test]
    fn test_retry_after_rate_limit() {
        let err = mock_error(StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(err.retry_after(0, 3), Some(Duration::from_secs(1)));
        assert_eq!(err.retry_after(1, 3), Some(Duration::from_secs(4)));
        assert_eq!(err.retry_after(2, 3), Some(Duration::from_secs(12)));
    }

    #[test]
    fn test_retry_after_exhausted() {
        let err = mock_error(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.retry_after(3, 3), None);
    }

    #[test]
    fn test_cbor_roundtrip_commit_request() {
        let req = jj_edenapi_types::wire::commit::WireReadCommitRequest {
            repo: "testrepo".to_string(),
            commit_id: vec![1, 2, 3, 4],
        };
        let cbor = serde_cbor::to_vec(&req).unwrap();
        let decoded: jj_edenapi_types::wire::commit::WireReadCommitRequest =
            serde_cbor::from_slice(&cbor).unwrap();
        assert_eq!(req.repo, decoded.repo);
        assert_eq!(req.commit_id, decoded.commit_id);
    }

    #[test]
    fn test_cbor_roundtrip_tree_response() {
        let tree = jj_edenapi_types::wire::tree::WireJjTree {
            tree_id: vec![5, 6, 7, 8],
            entries: std::collections::HashMap::new(),
        };
        let resp = jj_edenapi_types::wire::tree::WireReadTreeResponse {
            tree: Some(tree),
        };
        let cbor = serde_cbor::to_vec(&resp).unwrap();
        let decoded: jj_edenapi_types::wire::tree::WireReadTreeResponse =
            serde_cbor::from_slice(&cbor).unwrap();
        assert!(decoded.tree.is_some());
        assert_eq!(decoded.tree.unwrap().tree_id, vec![5, 6, 7, 8]);
    }

    #[test]
    fn test_builder_missing_server_url_fails() {
        let result = Builder::new()
            .repo_name("testrepo")
            .build();
        assert!(matches!(result, Err(JjRemoteApiError::BadConfig(ConfigError::Missing(k))) if k == "jjapi.url"));
    }

    #[test]
    fn test_builder_missing_repo_name_fails() {
        let result = Builder::new()
            .server_url("https://example.com".parse().unwrap())
            .build();
        assert!(matches!(result, Err(JjRemoteApiError::BadConfig(ConfigError::Missing(k))) if k == "jjapi.reponame"));
    }

    #[test]
    fn test_url_has_no_trailing_slash() {
        let client = Builder::new()
            .server_url("https://example.com".parse().unwrap())
            .repo_name("myrepo")
            .build()
            .unwrap();
        let url = client.build_url("commit/read").unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/myrepo/jjapi/commit/read"
        );
    }

    #[test]
    fn test_url_existing_base_trailing_slash_preserved_without_endpoint_slash() {
        let client = Builder::new()
            .server_url("https://example.com/".parse().unwrap())
            .repo_name("myrepo")
            .build()
            .unwrap();
        let url = client.build_url("commit/read").unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/myrepo/jjapi/commit/read"
        );
    }

    #[test]
    fn test_url_percent_encodes_repo_name_as_single_path_segment() {
        let client = Builder::new()
            .server_url("https://example.com/".parse().unwrap())
            .repo_name("noumena/github-parity-source")
            .build()
            .unwrap();
        let url = client.build_url("opstore/opheads/cas").unwrap();
        assert_eq!(
            url.as_str(),
            "https://example.com/noumena%2Fgithub-parity-source/jjapi/opstore/opheads/cas"
        );
    }
}
