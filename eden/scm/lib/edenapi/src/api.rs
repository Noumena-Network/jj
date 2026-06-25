/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! `JjRemoteApi` async trait -- the client-side contract every HTTP or mock client implements.
//!
//! Methods matching our server handlers in `eden/mononoke/servers/jjapi/jjapi_service`.

use async_trait::async_trait;
use jj_edenapi_types::wire::bookmark::WireCreateBookmarkRequest;
use jj_edenapi_types::wire::bookmark::WireCreateBookmarkResponse;
use jj_edenapi_types::wire::bookmark::WireDeleteBookmarkRequest;
use jj_edenapi_types::wire::bookmark::WireDeleteBookmarkResponse;
use jj_edenapi_types::wire::commit::WireReadCommitRequest;
use jj_edenapi_types::wire::commit::WireReadCommitResponse;
use jj_edenapi_types::wire::commit::WireWriteCommitRequest;
use jj_edenapi_types::wire::commit::WireWriteCommitResponse;
use jj_edenapi_types::wire::tree::WireReadTreeRequest;
use jj_edenapi_types::wire::tree::WireReadTreeResponse;
use jj_edenapi_types::wire::tree::WireWriteTreeRequest;
use jj_edenapi_types::wire::tree::WireWriteTreeResponse;
use jj_edenapi_types::wire::file::WireReadFileRequest;
use jj_edenapi_types::wire::file::WireReadFileResponse;
use jj_edenapi_types::wire::file::WireReadSymlinkRequest;
use jj_edenapi_types::wire::file::WireReadSymlinkResponse;
use jj_edenapi_types::wire::file::WireWriteFileRequest;
use jj_edenapi_types::wire::file::WireWriteFileResponse;
use jj_edenapi_types::wire::file::WireWriteSymlinkRequest;
use jj_edenapi_types::wire::file::WireWriteSymlinkResponse;
use jj_edenapi_types::wire::bookmark::WireListBookmarksRequest;
use jj_edenapi_types::wire::bookmark::WireListBookmarksResponse;
use jj_edenapi_types::wire::bookmark::WireMoveBookmarkRequest;
use jj_edenapi_types::wire::bookmark::WireMoveBookmarkResponse;
use jj_edenapi_types::wire::bookmark::WireResolveBookmarkRequest;
use jj_edenapi_types::wire::bookmark::WireResolveBookmarkResponse;
use jj_edenapi_types::wire::opstore::WireCompareAndSwapOpHeadsRequest;
use jj_edenapi_types::wire::opstore::WireCompareAndSwapOpHeadsResponse;
use jj_edenapi_types::wire::opstore::WireReadOpHeadsRequest;
use jj_edenapi_types::wire::opstore::WireReadOpHeadsResponse;
use jj_edenapi_types::wire::opstore::WireReadOperationRequest;
use jj_edenapi_types::wire::opstore::WireReadOperationResponse;
use jj_edenapi_types::wire::opstore::WireReadViewRequest;
use jj_edenapi_types::wire::opstore::WireReadViewResponse;
use jj_edenapi_types::wire::opstore::WireWriteOperationRequest;
use jj_edenapi_types::wire::opstore::WireWriteOperationResponse;
use jj_edenapi_types::wire::opstore::WireWriteViewRequest;
use jj_edenapi_types::wire::opstore::WireWriteViewResponse;
use jj_edenapi_types::wire::workspace::WireDeleteWorkspaceRequest;
use jj_edenapi_types::wire::workspace::WireDeleteWorkspaceResponse;
use jj_edenapi_types::wire::workspace::WireGetWorkspaceRequest;
use jj_edenapi_types::wire::workspace::WireGetWorkspaceResponse;
use jj_edenapi_types::wire::workspace::WireListWorkspacesRequest;
use jj_edenapi_types::wire::workspace::WireListWorkspacesResponse;
use jj_edenapi_types::wire::workspace::WirePutWorkspaceRequest;
use jj_edenapi_types::wire::workspace::WirePutWorkspaceResponse;

use crate::errors::JjRemoteApiError;
use crate::response::Response;
use crate::response::ResponseMeta;

#[async_trait]
pub trait JjRemoteApi: Send + Sync + 'static {
    /// Base server URL, if known.
    fn url(&self) -> Option<String> {
        None
    }

    /// Health check. Returns "I_AM_ALIVE" on success.
    async fn health(&self) -> Result<ResponseMeta, JjRemoteApiError>;

    // -- commit --

    /// Read a commit from the server.
    async fn read_commit(
        &self,
        request: WireReadCommitRequest,
    ) -> Result<Response<WireReadCommitResponse>, JjRemoteApiError>;

    /// Write a commit to the server.
    async fn write_commit(
        &self,
        request: WireWriteCommitRequest,
    ) -> Result<Response<WireWriteCommitResponse>, JjRemoteApiError>;

    // -- tree --

    /// Read a tree from the server by tree-id.
    async fn read_tree(
        &self,
        request: WireReadTreeRequest,
    ) -> Result<Response<WireReadTreeResponse>, JjRemoteApiError>;

    /// Write a tree and receive back the content-hashed tree-id.
    async fn write_tree(
        &self,
        request: WireWriteTreeRequest,
    ) -> Result<Response<WireWriteTreeResponse>, JjRemoteApiError>;

    // -- file --

    /// Read a raw file by file-id.
    async fn read_file(
        &self,
        request: WireReadFileRequest,
    ) -> Result<Response<WireReadFileResponse>, JjRemoteApiError>;

    /// Write a raw file and receive back the content-hashed file-id.
    async fn write_file(
        &self,
        request: WireWriteFileRequest,
    ) -> Result<Response<WireWriteFileResponse>, JjRemoteApiError>;

    /// Read a symlink target by file-id.
    async fn read_symlink(
        &self,
        request: WireReadSymlinkRequest,
    ) -> Result<Response<WireReadSymlinkResponse>, JjRemoteApiError>;

    /// Write a symlink target and receive back the content-hashed file-id.
    async fn write_symlink(
        &self,
        request: WireWriteSymlinkRequest,
    ) -> Result<Response<WireWriteSymlinkResponse>, JjRemoteApiError>;

    // -- bookmark --

    /// Resolve a single bookmark name to a commit id.
    async fn resolve_bookmark(
        &self,
        request: WireResolveBookmarkRequest,
    ) -> Result<Response<WireResolveBookmarkResponse>, JjRemoteApiError>;

    /// List bookmarks matching an optional prefix.
    async fn list_bookmarks(
        &self,
        request: WireListBookmarksRequest,
    ) -> Result<Response<WireListBookmarksResponse>, JjRemoteApiError>;

    /// Create a bookmark at the requested commit id.
    async fn create_bookmark(
        &self,
        request: WireCreateBookmarkRequest,
    ) -> Result<Response<WireCreateBookmarkResponse>, JjRemoteApiError>;

    /// Move an existing bookmark from one commit id to another.
    async fn move_bookmark(
        &self,
        request: WireMoveBookmarkRequest,
    ) -> Result<Response<WireMoveBookmarkResponse>, JjRemoteApiError>;

    /// Delete an existing bookmark at the requested commit id.
    async fn delete_bookmark(
        &self,
        request: WireDeleteBookmarkRequest,
    ) -> Result<Response<WireDeleteBookmarkResponse>, JjRemoteApiError>;

    // -- opstore --

    async fn read_view(
        &self,
        request: WireReadViewRequest,
    ) -> Result<Response<WireReadViewResponse>, JjRemoteApiError>;

    async fn write_view(
        &self,
        request: WireWriteViewRequest,
    ) -> Result<Response<WireWriteViewResponse>, JjRemoteApiError>;

    async fn read_operation(
        &self,
        request: WireReadOperationRequest,
    ) -> Result<Response<WireReadOperationResponse>, JjRemoteApiError>;

    async fn write_operation(
        &self,
        request: WireWriteOperationRequest,
    ) -> Result<Response<WireWriteOperationResponse>, JjRemoteApiError>;

    async fn read_op_heads(
        &self,
        request: WireReadOpHeadsRequest,
    ) -> Result<Response<WireReadOpHeadsResponse>, JjRemoteApiError>;

    async fn compare_and_swap_op_heads(
        &self,
        request: WireCompareAndSwapOpHeadsRequest,
    ) -> Result<Response<WireCompareAndSwapOpHeadsResponse>, JjRemoteApiError>;

    // -- workspace --

    async fn get_workspace(
        &self,
        request: WireGetWorkspaceRequest,
    ) -> Result<Response<WireGetWorkspaceResponse>, JjRemoteApiError>;

    async fn put_workspace(
        &self,
        request: WirePutWorkspaceRequest,
    ) -> Result<Response<WirePutWorkspaceResponse>, JjRemoteApiError>;

    async fn list_workspaces(
        &self,
        request: WireListWorkspacesRequest,
    ) -> Result<Response<WireListWorkspacesResponse>, JjRemoteApiError>;

    async fn delete_workspace(
        &self,
        request: WireDeleteWorkspaceRequest,
    ) -> Result<Response<WireDeleteWorkspaceResponse>, JjRemoteApiError>;
}
