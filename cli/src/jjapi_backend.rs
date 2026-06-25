/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! HTTP-backed `jj_lib::backend::Backend` implementation delegating to
//! `jj_edenapi::JjRemoteApi`.
//!
//! This is the product auth path: GHES Bearer tokens over HTTP/CBOR to jjapi.
//! Not Thrift, not mTLS.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use futures::FutureExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::stream;
use futures::AsyncRead;
use futures::AsyncReadExt;
use futures::StreamExt as _;
use futures::io::AllowStdIo;

use once_cell::sync::Lazy;

static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Runtime::new().expect("failed to initialize the JJAPI tokio runtime")
});

use jj_edenapi::JjRemoteApi;
use jj_edenapi_types::wire::commit::WireJjCommit;
use jj_edenapi_types::wire::commit::WireJjFileAddition;
use jj_edenapi_types::wire::commit::WireJjFileChange;
use jj_edenapi_types::wire::commit::WireJjFileDeletion;
use jj_edenapi_types::wire::commit::WireJjFileType;
use jj_edenapi_types::wire::commit::WireJjSecureSig;
use jj_edenapi_types::wire::commit::WireJjSignature;
use jj_edenapi_types::wire::commit::WireReadCommitRequest;
use jj_edenapi_types::wire::commit::WireWriteCommitRequest;
use jj_edenapi_types::wire::file::WireReadFileRequest;
use jj_edenapi_types::wire::file::WireReadSymlinkRequest;
use jj_edenapi_types::wire::file::WireWriteFileRequest;
use jj_edenapi_types::wire::file::WireWriteSymlinkRequest;
use jj_edenapi_types::wire::tree::WireJjTree;
use jj_edenapi_types::wire::tree::WireJjTreeEntry;
use jj_edenapi_types::wire::tree::WireJjTreeEntryDirectory;
use jj_edenapi_types::wire::tree::WireJjTreeEntryFile;
use jj_edenapi_types::wire::tree::WireJjTreeEntrySymlink;
use jj_edenapi_types::wire::tree::WireReadTreeRequest;
use jj_edenapi_types::wire::tree::WireWriteTreeRequest;

use jj_lib::backend::Backend;
use jj_lib::backend::BackendError;
use jj_lib::backend::BackendResult;
use jj_lib::backend::ChangeId;
use jj_lib::backend::Commit;
use jj_lib::backend::CommitId;
use jj_lib::backend::CopyHistory;
use jj_lib::backend::CopyId;
use jj_lib::backend::CopyRecord;
use jj_lib::backend::FileId;
use jj_lib::backend::RelatedCopy;
use jj_lib::backend::SecureSig;
use jj_lib::backend::Signature;
use jj_lib::backend::SigningFn;
use jj_lib::backend::SymlinkId;
use jj_lib::backend::Timestamp;
use jj_lib::backend::Tree;
use jj_lib::backend::TreeId;
use jj_lib::backend::TreeValue;
use jj_lib::backend::make_root_commit;
use jj_lib::index::Index;
use jj_lib::merge::Merge;
use jj_lib::object_id::ObjectId as _;
use jj_lib::op_store::RootOperationData;
use jj_lib::op_store::View;
use jj_lib::repo::StoreFactories;
use jj_lib::repo_path::RepoPath;
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::repo_path::RepoPathComponent;
use jj_lib::repo_path::RepoPathComponentBuf;
use jj_lib::settings::UserSettings;

/// HTTP-backed `Backend` delegating to a `JjRemoteApi` implementation.
pub struct JjapiBackend {
    repo_name: String,
    root_commit_id: CommitId,
    root_change_id: ChangeId,
    empty_tree_id: TreeId,
    client: Arc<dyn JjRemoteApi>,
}

impl fmt::Debug for JjapiBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JjapiBackend")
            .field("repo_name", &self.repo_name)
            .finish_non_exhaustive()
    }
}

impl JjapiBackend {
    pub fn new(
        repo_name: String,
        root_commit_id: CommitId,
        root_change_id: ChangeId,
        empty_tree_id: TreeId,
        client: Arc<dyn JjRemoteApi>,
    ) -> Self {
        Self {
            repo_name,
            root_commit_id,
            root_change_id,
            empty_tree_id,
            client,
        }
    }

    pub fn name() -> &'static str {
        "jjapi"
    }

    fn api_err(&self, e: jj_edenapi::JjRemoteApiError, object_type: &str, hash: &str) -> BackendError {
        BackendError::ReadObject {
            object_type: object_type.to_string(),
            hash: hash.to_string(),
            source: Box::new(e),
        }
    }

    fn block_on<F: std::future::Future>(&self, f: F) -> F::Output {
        RUNTIME.block_on(f)
    }

    fn invalid_data(message: impl Into<String>) -> BackendError {
        BackendError::Other(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.into(),
        )))
    }

    fn child_path(
        parent: &RepoPath,
        name: &RepoPathComponent,
    ) -> BackendResult<RepoPathBuf> {
        RepoPathBuf::from_internal_string(format!(
            "{}{}",
            parent.to_internal_dir_string(),
            name.as_internal_str()
        ))
        .map_err(|err| Self::invalid_data(format!("invalid repository path: {err}")))
    }

    async fn read_tree_remote(&self, path: &RepoPath, id: &TreeId) -> BackendResult<Tree> {
        if id == &self.empty_tree_id {
            return Ok(Tree::default());
        }

        let req = WireReadTreeRequest {
            repo: self.repo_name.clone(),
            tree_id: id.as_bytes().to_vec(),
        };
        let resp = self
            .client
            .read_tree(req)
            .await
            .map_err(|e| self.api_err(e, "tree", &id.hex()))?;
        let entries = resp.flatten().await.map_err(|e| self.api_err(e, "tree", &id.hex()))?;
        let entry = entries.into_iter().next().ok_or_else(|| BackendError::ObjectNotFound {
            object_type: "tree".to_string(),
            hash: id.hex(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("empty response for tree at {}", path.as_internal_file_string()),
            )),
        })?;
        let wire_tree = entry.tree.ok_or_else(|| BackendError::ObjectNotFound {
            object_type: "tree".to_string(),
            hash: id.hex(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("tree not found at {}", path.as_internal_file_string()),
            )),
        })?;
        tree_from_wire(wire_tree)
    }

    async fn read_file_bytes_remote(&self, path: &RepoPath, id: &FileId) -> BackendResult<Vec<u8>> {
        let req = WireReadFileRequest {
            repo: self.repo_name.clone(),
            file_id: id.as_bytes().to_vec(),
        };
        let resp = self
            .client
            .read_file(req)
            .await
            .map_err(|e| self.api_err(e, "file", &id.hex()))?;
        let entries = resp.flatten().await.map_err(|e| self.api_err(e, "file", &id.hex()))?;
        let entry = entries.into_iter().next().ok_or_else(|| BackendError::ObjectNotFound {
            object_type: "file".to_string(),
            hash: id.hex(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("empty response for file at {}", path.as_internal_file_string()),
            )),
        })?;
        entry.content.ok_or_else(|| BackendError::ObjectNotFound {
            object_type: "file".to_string(),
            hash: id.hex(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("file content not found at {}", path.as_internal_file_string()),
            )),
        })
    }

    async fn read_symlink_remote(&self, path: &RepoPath, id: &SymlinkId) -> BackendResult<String> {
        let req = WireReadSymlinkRequest {
            repo: self.repo_name.clone(),
            file_id: id.as_bytes().to_vec(),
        };
        let resp = self
            .client
            .read_symlink(req)
            .await
            .map_err(|e| self.api_err(e, "symlink", &id.hex()))?;
        let entries = resp.flatten().await.map_err(|e| self.api_err(e, "symlink", &id.hex()))?;
        let entry = entries.into_iter().next().ok_or_else(|| BackendError::ObjectNotFound {
            object_type: "symlink".to_string(),
            hash: id.hex(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("empty response for symlink at {}", path.as_internal_file_string()),
            )),
        })?;
        entry.target.ok_or_else(|| BackendError::ObjectNotFound {
            object_type: "symlink".to_string(),
            hash: id.hex(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("symlink target not found at {}", path.as_internal_file_string()),
            )),
        })
    }

    async fn read_commit_remote(&self, id: &CommitId) -> BackendResult<Commit> {
        if id == &self.root_commit_id {
            return Ok(make_root_commit(
                self.root_change_id.clone(),
                self.empty_tree_id.clone(),
            ));
        }

        let req = WireReadCommitRequest {
            repo: self.repo_name.clone(),
            commit_id: id.as_bytes().to_vec(),
        };
        let resp = self
            .client
            .read_commit(req)
            .await
            .map_err(|e| self.api_err(e, "commit", &id.hex()))?;
        let entries = resp
            .flatten()
            .await
            .map_err(|e| self.api_err(e, "commit", &id.hex()))?;
        let entry = entries.into_iter().next().ok_or_else(|| BackendError::ObjectNotFound {
            object_type: "commit".to_string(),
            hash: id.hex(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "empty response",
            )),
        })?;
        let wire_commit = entry.commit.ok_or_else(|| BackendError::ObjectNotFound {
            object_type: "commit".to_string(),
            hash: id.hex(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "commit not found",
            )),
        })?;
        Ok(commit_from_wire(wire_commit))
    }

    async fn wire_file_changes_for_commit(
        &self,
        commit: &Commit,
    ) -> BackendResult<Vec<WireJjFileChange>> {
        let root_tree_id = commit.root_tree.as_resolved().ok_or_else(|| {
            BackendError::Unsupported(
                "JJAPI commit writes require a resolved root tree; conflicted root trees are not representable as Mononoke Bonsai file_changes yet".to_string(),
            )
        })?;

        let parent_tree_id = match commit.parents.as_slice() {
            [] => self.empty_tree_id.clone(),
            [parent] => {
                let parent_commit = self.read_commit_remote(parent).await?;
                parent_commit.root_tree.as_resolved().cloned().ok_or_else(|| {
                    BackendError::Unsupported(
                        "JJAPI commit writes require resolved parent root trees; conflicted parent root trees are not representable as Mononoke Bonsai file_changes yet".to_string(),
                    )
                })?
            }
            parents => {
                let mut parent_tree_ids = Vec::with_capacity(parents.len());
                for parent in parents {
                    let parent_commit = self.read_commit_remote(parent).await?;
                    let parent_tree_id = parent_commit.root_tree.as_resolved().cloned().ok_or_else(|| {
                        BackendError::Unsupported(
                            "JJAPI merge commit writes require resolved parent root trees; conflicted parent root trees are not representable as Mononoke Bonsai file_changes yet".to_string(),
                        )
                    })?;
                    parent_tree_ids.push(parent_tree_id);
                }
                if parent_tree_ids.iter().all(|id| id == root_tree_id) {
                    return Ok(Vec::new());
                }
                let first = parent_tree_ids[0].clone();
                if parent_tree_ids.iter().all(|id| id == &first) {
                    first
                } else {
                    return Err(BackendError::Unsupported(
                        "JJAPI merge commit writes with divergent parent trees require a server-side merge-tree to Bonsai file_changes derivation; refusing to write lossy file_changes".to_string(),
                    ));
                }
            }
        };

        self.diff_tree_file_changes(RepoPathBuf::root(), &parent_tree_id, root_tree_id)
            .await
    }

    fn diff_tree_file_changes<'a>(
        &'a self,
        path: RepoPathBuf,
        before: &'a TreeId,
        after: &'a TreeId,
    ) -> BoxFuture<'a, BackendResult<Vec<WireJjFileChange>>> {
        async move {
            if before == after {
                return Ok(Vec::new());
            }

            let before_tree = self.read_tree_remote(path.as_ref(), before).await?;
            let after_tree = self.read_tree_remote(path.as_ref(), after).await?;

            let before_entries: BTreeMap<RepoPathComponentBuf, TreeValue> = before_tree
                .entries()
                .map(|entry| (entry.name().to_owned(), entry.value().clone()))
                .collect();
            let after_entries: BTreeMap<RepoPathComponentBuf, TreeValue> = after_tree
                .entries()
                .map(|entry| (entry.name().to_owned(), entry.value().clone()))
                .collect();
            let names: BTreeSet<RepoPathComponentBuf> = before_entries
                .keys()
                .chain(after_entries.keys())
                .cloned()
                .collect();

            let mut changes = Vec::new();
            for name in names {
                let child_path = Self::child_path(path.as_ref(), name.as_ref())?;
                match (before_entries.get(&name), after_entries.get(&name)) {
                    (None, Some(after_value)) => {
                        changes.extend(self.add_value_file_changes(child_path, after_value).await?);
                    }
                    (Some(before_value), None) => {
                        changes.extend(self.delete_value_file_changes(child_path, before_value).await?);
                    }
                    (Some(before_value), Some(after_value)) if before_value == after_value => {}
                    (Some(TreeValue::Tree(before_id)), Some(TreeValue::Tree(after_id))) => {
                        changes.extend(
                            self.diff_tree_file_changes(child_path, before_id, after_id)
                                .await?,
                        );
                    }
                    (Some(before_value), Some(after_value)) => {
                        changes.extend(self.delete_value_file_changes(child_path.clone(), before_value).await?);
                        changes.extend(self.add_value_file_changes(child_path, after_value).await?);
                    }
                    (None, None) => {}
                }
            }

            Ok(changes)
        }
        .boxed()
    }

    fn add_value_file_changes<'a>(
        &'a self,
        path: RepoPathBuf,
        value: &'a TreeValue,
    ) -> BoxFuture<'a, BackendResult<Vec<WireJjFileChange>>> {
        async move {
            match value {
                TreeValue::File {
                    id,
                    executable,
                    copy_id: _,
                } => {
                    let content = self.read_file_bytes_remote(path.as_ref(), id).await?;
                    let file_type = if *executable {
                        WireJjFileType::Executable
                    } else {
                        WireJjFileType::Regular
                    };
                    Ok(vec![WireJjFileChange::Addition(WireJjFileAddition {
                        path: path.as_internal_file_string().to_string(),
                        file_type,
                        content,
                    })])
                }
                TreeValue::Symlink(id) => {
                    let target = self.read_symlink_remote(path.as_ref(), id).await?;
                    Ok(vec![WireJjFileChange::Addition(WireJjFileAddition {
                        path: path.as_internal_file_string().to_string(),
                        file_type: WireJjFileType::Symlink,
                        content: target.into_bytes(),
                    })])
                }
                TreeValue::Tree(id) => {
                    self.add_tree_file_changes(path, id).await
                }
                TreeValue::GitSubmodule(_) => Err(BackendError::Unsupported(
                    "JJAPI commit writes cannot serialize Git submodule additions into Mononoke Bonsai file_changes yet".to_string(),
                )),
            }
        }
        .boxed()
    }

    fn add_tree_file_changes<'a>(
        &'a self,
        path: RepoPathBuf,
        id: &'a TreeId,
    ) -> BoxFuture<'a, BackendResult<Vec<WireJjFileChange>>> {
        async move {
            let tree = self.read_tree_remote(path.as_ref(), id).await?;
            let mut changes = Vec::new();
            for entry in tree.entries() {
                let child_path = Self::child_path(path.as_ref(), entry.name())?;
                changes.extend(
                    self.add_value_file_changes(child_path, entry.value()).await?,
                );
            }
            Ok(changes)
        }
        .boxed()
    }

    fn delete_value_file_changes<'a>(
        &'a self,
        path: RepoPathBuf,
        value: &'a TreeValue,
    ) -> BoxFuture<'a, BackendResult<Vec<WireJjFileChange>>> {
        async move {
            match value {
                TreeValue::Tree(id) => self.delete_tree_file_changes(path, id).await,
                _ => Ok(vec![WireJjFileChange::Deletion(WireJjFileDeletion {
                    path: path.as_internal_file_string().to_string(),
                })]),
            }
        }
        .boxed()
    }

    fn delete_tree_file_changes<'a>(
        &'a self,
        path: RepoPathBuf,
        id: &'a TreeId,
    ) -> BoxFuture<'a, BackendResult<Vec<WireJjFileChange>>> {
        async move {
            let tree = self.read_tree_remote(path.as_ref(), id).await?;
            let mut changes = Vec::new();
            for entry in tree.entries() {
                let child_path = Self::child_path(path.as_ref(), entry.name())?;
                changes.extend(
                    self.delete_value_file_changes(child_path, entry.value())
                        .await?,
                );
            }
            Ok(changes)
        }
        .boxed()
    }
}

fn load_jjapi_config(
    settings: &UserSettings,
) -> Result<(String, CommitId, ChangeId, TreeId, jj_lib::ref_name::WorkspaceNameBuf, Arc<dyn JjRemoteApi>), jj_lib::backend::BackendLoadError> {
    use jj_lib::config::ConfigGetResultExt as _;

    let repo_name: String = settings
        .get("jjapi.reponame")
        .map_err(|e| jj_lib::backend::BackendLoadError(Box::new(e)))?;
    let http_url: String = settings
        .get("jjapi.url")
        .map_err(|e| jj_lib::backend::BackendLoadError(Box::new(e)))?;
    let root_commit_id = settings
        .get("jjapi.root_commit_id")
        .optional()
        .ok()
        .flatten()
        .and_then(|hex: String| CommitId::try_from_hex(&hex))
        .unwrap_or_else(|| CommitId::from_bytes(&[0; 32]));
    let empty_tree_id = settings
        .get("jjapi.empty_tree_id")
        .map_err(|e| jj_lib::backend::BackendLoadError(Box::new(e)))
        .and_then(|hex: String| {
            TreeId::try_from_hex(&hex).ok_or_else(|| {
                jj_lib::backend::BackendLoadError(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "invalid jjapi.empty_tree_id",
                )))
            })
        })?;
    let workspace_name = settings
        .get::<String>("jjapi.workspace")
        .map_err(|e| jj_lib::backend::BackendLoadError(Box::new(e)))
        .map(jj_lib::ref_name::WorkspaceNameBuf::from)?;
    let client = jj_edenapi::Builder::new()
        .server_url(http_url.parse().map_err(|e| jj_lib::backend::BackendLoadError(Box::new(e)))?)
        .repo_name(&repo_name)
        .build()
        .map_err(|e| jj_lib::backend::BackendLoadError(Box::new(e)))?;

    Ok((
        repo_name,
        root_commit_id,
        ChangeId::from_bytes(&[0; 16]),
        empty_tree_id,
        workspace_name,
        Arc::new(client),
    ))
}

pub fn jjapi_store_factories() -> StoreFactories {
    let mut factories = StoreFactories::empty();

    factories.add_backend("jjapi", Box::new(|settings, _store_path| {
        let (repo_name, root_commit_id, root_change_id, empty_tree_id, _workspace, client) =
            load_jjapi_config(settings)?;
        Ok(Box::new(JjapiBackend::new(
            repo_name,
            root_commit_id,
            root_change_id,
            empty_tree_id,
            client,
        )))
    }));

    factories.add_op_store("jjapi-opstore", Box::new(|settings, _store_path, root_data| {
        let (repo_name, root_commit_id, _root_change_id, _empty_tree_id, workspace, client) =
            load_jjapi_config(settings)?;
        let root_view = View::make_root(root_commit_id);
        let root_view_id = jj_lib::op_store::ViewId::from_bytes(
            jj_lib::content_hash::blake2b_hash(&root_view).to_vec().as_slice(),
        );
        let root_op = jj_lib::op_store::Operation::make_root(root_view_id.clone());
        let root_operation_id = jj_lib::op_store::OperationId::from_bytes(
            jj_lib::content_hash::blake2b_hash(&root_op).to_vec().as_slice(),
        );
        Ok(Box::new(crate::jjapi_op_store::JjapiOpStore::new(
            repo_name,
            workspace,
            root_data,
            root_operation_id,
            root_view_id,
            client,
        )))
    }));

    factories.add_op_heads_store("jjapi-opheads", Box::new(|settings, _store_path| {
        let (repo_name, root_commit_id, _root_change_id, _empty_tree_id, workspace, client) =
            load_jjapi_config(settings)?;
        let root_data = RootOperationData { root_commit_id: root_commit_id.clone() };
        let root_view = View::make_root(root_commit_id);
        let root_view_id = jj_lib::op_store::ViewId::from_bytes(
            jj_lib::content_hash::blake2b_hash(&root_view).to_vec().as_slice(),
        );
        let root_op = jj_lib::op_store::Operation::make_root(root_view_id.clone());
        let root_operation_id = jj_lib::op_store::OperationId::from_bytes(
            jj_lib::content_hash::blake2b_hash(&root_op).to_vec().as_slice(),
        );
        Ok(Box::new(crate::jjapi_op_store::JjapiOpStore::new(
            repo_name,
            workspace,
            root_data,
            root_operation_id,
            root_view_id,
            client,
        )))
    }));

    factories
}

#[async_trait]
impl Backend for JjapiBackend {
    fn name(&self) -> &str {
        Self::name()
    }

    fn commit_id_length(&self) -> usize {
        self.root_commit_id.as_bytes().len()
    }

    fn change_id_length(&self) -> usize {
        self.root_change_id.as_bytes().len()
    }

    fn root_commit_id(&self) -> &CommitId {
        &self.root_commit_id
    }

    fn root_change_id(&self) -> &ChangeId {
        &self.root_change_id
    }

    fn empty_tree_id(&self) -> &TreeId {
        &self.empty_tree_id
    }

    fn concurrency(&self) -> usize {
        100
    }

    async fn read_file(
        &self,
        _path: &RepoPath,
        id: &FileId,
    ) -> BackendResult<Pin<Box<dyn AsyncRead + Send>>> {
        self.block_on(async {
            let req = WireReadFileRequest {
                repo: self.repo_name.clone(),
                file_id: id.as_bytes().to_vec(),
            };
            let resp = self
                .client
                .read_file(req)
                .await
                .map_err(|e| self.api_err(e, "file", &id.hex()))?;
            let entries = resp.flatten().await.map_err(|e| self.api_err(e, "file", &id.hex()))?;
            let entry = entries.into_iter().next().ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "file".to_string(),
                hash: id.hex(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "empty response",
                )),
            })?;
            let content = entry.content.ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "file".to_string(),
                hash: id.hex(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "file not found",
                )),
            })?;
            let cursor = std::io::Cursor::new(content);
            let ret: Pin<Box<dyn AsyncRead + Send>> = Box::pin(AllowStdIo::new(cursor));
            Ok(ret)
        })
    }

    async fn write_file(
        &self,
        _path: &RepoPath,
        contents: &mut (dyn AsyncRead + Send + Unpin),
    ) -> BackendResult<FileId> {
        self.block_on(async {
            let mut buf = Vec::new();
            contents
                .read_to_end(&mut buf)
                .await
                .map_err(|e| BackendError::WriteObject {
                    object_type: "file",
                    source: Box::new(e),
                })?;
            let req = WireWriteFileRequest {
                repo: self.repo_name.clone(),
                content: buf,
            };
            let resp = self
                .client
                .write_file(req)
                .await
                .map_err(|e| BackendError::WriteObject {
                    object_type: "file",
                    source: Box::new(e),
                })?;
            let entries = resp.flatten().await.map_err(|e| BackendError::WriteObject {
                object_type: "file",
                source: Box::new(e),
            })?;
            let entry = entries.into_iter().next().ok_or_else(|| BackendError::WriteObject {
                object_type: "file",
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "empty response",
                )),
            })?;
            Ok(FileId::from_bytes(&entry.file_id))
        })
    }

    async fn read_symlink(&self, _path: &RepoPath, id: &SymlinkId) -> BackendResult<String> {
        self.block_on(async {
            let req = WireReadSymlinkRequest {
                repo: self.repo_name.clone(),
                file_id: id.as_bytes().to_vec(),
            };
            let resp = self
                .client
                .read_symlink(req)
                .await
                .map_err(|e| self.api_err(e, "symlink", &id.hex()))?;
            let entries = resp.flatten().await.map_err(|e| self.api_err(e, "symlink", &id.hex()))?;
            let entry = entries.into_iter().next().ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "symlink".to_string(),
                hash: id.hex(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "empty response",
                )),
            })?;
            let target = entry.target.ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "symlink".to_string(),
                hash: id.hex(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "symlink not found",
                )),
            })?;
            Ok(target)
        })
    }

    async fn write_symlink(&self, _path: &RepoPath, target: &str) -> BackendResult<SymlinkId> {
        self.block_on(async {
            let req = WireWriteSymlinkRequest {
                repo: self.repo_name.clone(),
                target: target.to_string(),
            };
            let resp = self
                .client
                .write_symlink(req)
                .await
                .map_err(|e| BackendError::WriteObject {
                    object_type: "symlink",
                    source: Box::new(e),
                })?;
            let entries = resp.flatten().await.map_err(|e| BackendError::WriteObject {
                object_type: "symlink",
                source: Box::new(e),
            })?;
            let entry = entries.into_iter().next().ok_or_else(|| BackendError::WriteObject {
                object_type: "symlink",
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "empty response",
                )),
            })?;
            Ok(SymlinkId::from_bytes(&entry.file_id))
        })
    }

    async fn read_copy(&self, _id: &CopyId) -> BackendResult<CopyHistory> {
        Err(BackendError::Unsupported(
            "copy tracking not supported over HTTP".to_string(),
        ))
    }

    async fn write_copy(&self, _copy: &CopyHistory) -> BackendResult<CopyId> {
        Err(BackendError::Unsupported(
            "copy tracking not supported over HTTP".to_string(),
        ))
    }

    async fn get_related_copies(&self, _copy_id: &CopyId) -> BackendResult<Vec<RelatedCopy>> {
        Err(BackendError::Unsupported(
            "copy tracking not supported over HTTP".to_string(),
        ))
    }

    async fn read_tree(&self, _path: &RepoPath, id: &TreeId) -> BackendResult<Tree> {
        if id == &self.empty_tree_id {
            return Ok(Tree::default());
        }
        self.block_on(async {
            let req = WireReadTreeRequest {
                repo: self.repo_name.clone(),
                tree_id: id.as_bytes().to_vec(),
            };
            let resp = self
                .client
                .read_tree(req)
                .await
                .map_err(|e| self.api_err(e, "tree", &id.hex()))?;
            let entries = resp.flatten().await.map_err(|e| self.api_err(e, "tree", &id.hex()))?;
            let entry = entries.into_iter().next().ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "tree".to_string(),
                hash: id.hex(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "empty response",
                )),
            })?;
            let wire_tree = entry.tree.ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "tree".to_string(),
                hash: id.hex(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "tree not found",
                )),
            })?;
            tree_from_wire(wire_tree)
        })
    }

    async fn write_tree(&self, _path: &RepoPath, contents: &Tree) -> BackendResult<TreeId> {
        self.block_on(async {
            let req = WireWriteTreeRequest {
                repo: self.repo_name.clone(),
                tree: tree_to_wire(contents),
            };
            let resp = self
                .client
                .write_tree(req)
                .await
                .map_err(|e| BackendError::WriteObject {
                    object_type: "tree",
                    source: Box::new(e),
                })?;
            let entries = resp.flatten().await.map_err(|e| BackendError::WriteObject {
                object_type: "tree",
                source: Box::new(e),
            })?;
            let entry = entries.into_iter().next().ok_or_else(|| BackendError::WriteObject {
                object_type: "tree",
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "empty response",
                )),
            })?;
            Ok(TreeId::from_bytes(&entry.tree_id))
        })
    }

    async fn read_commit(&self, id: &CommitId) -> BackendResult<Commit> {
        if id == &self.root_commit_id {
            return Ok(make_root_commit(
                self.root_change_id.clone(),
                self.empty_tree_id.clone(),
            ));
        }
        self.block_on(async {
            let req = WireReadCommitRequest {
                repo: self.repo_name.clone(),
                commit_id: id.as_bytes().to_vec(),
            };
            let resp = self
                .client
                .read_commit(req)
                .await
                .map_err(|e| self.api_err(e, "commit", &id.hex()))?;
            let entries = resp
                .flatten()
                .await
                .map_err(|e| self.api_err(e, "commit", &id.hex()))?;
            let entry = entries.into_iter().next().ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "commit".to_string(),
                hash: id.hex(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "empty response",
                )),
            })?;
            let wire_commit = entry.commit.ok_or_else(|| BackendError::ObjectNotFound {
                object_type: "commit".to_string(),
                hash: id.hex(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "commit not found",
                )),
            })?;
            Ok(commit_from_wire(wire_commit))
        })
    }

    async fn write_commit(
        &self,
        contents: Commit,
        _sign_with: Option<&mut SigningFn>,
    ) -> BackendResult<(CommitId, Commit)> {
        self.block_on(async {
            let mut commit_hash = jj_lib::content_hash::blake2b_hash(&contents).to_vec();
            commit_hash.truncate(32);
            let commit_id = CommitId::new(commit_hash);
            let file_changes = self.wire_file_changes_for_commit(&contents).await?;
            let req = WireWriteCommitRequest {
                repo: self.repo_name.clone(),
                commit: commit_to_wire(&contents, &commit_id, file_changes),
            };
            let resp = self
                .client
                .write_commit(req)
                .await
                .map_err(|e| BackendError::WriteObject {
                    object_type: "commit",
                    source: Box::new(e),
                })?;
            let entries = resp.flatten().await.map_err(|e| BackendError::WriteObject {
                object_type: "commit",
                source: Box::new(e),
            })?;
            let entry = entries.into_iter().next().ok_or_else(|| BackendError::WriteObject {
                object_type: "commit",
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "empty response",
                )),
            })?;
            let returned_commit_id = CommitId::from_bytes(&entry.commit_id);
            if returned_commit_id != commit_id {
                return Err(BackendError::WriteObject {
                    object_type: "commit",
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "remote returned commit id {}, expected {}",
                            returned_commit_id.hex(),
                            commit_id.hex()
                        ),
                    )),
                });
            }
            Ok((commit_id, contents))
        })
    }

    fn get_copy_records(
        &self,
        _paths: Option<&[RepoPathBuf]>,
        _root: &CommitId,
        _head: &CommitId,
    ) -> BackendResult<BoxStream<'_, BackendResult<CopyRecord>>> {
        Ok(stream::empty().boxed())
    }

    fn gc(&self, _index: &dyn Index, _keep_newer: SystemTime) -> BackendResult<()> {
        Ok(())
    }
}

// ----------------------------------------------------------------------
// Conversions: jj_lib domain types  <->  wire types
// ----------------------------------------------------------------------

fn commit_to_wire(
    commit: &Commit,
    commit_id: &CommitId,
    file_changes: Vec<WireJjFileChange>,
) -> WireJjCommit {
    let root_tree_bytes = commit
        .root_tree
        .as_resolved()
        .map(|id| id.as_bytes().to_vec())
        .unwrap_or_default();

    WireJjCommit {
        commit_id: commit_id.as_bytes().to_vec(),
        change_id: commit.change_id.as_bytes().to_vec(),
        parents: commit.parents.iter().map(|id| id.as_bytes().to_vec()).collect(),
        root_tree: root_tree_bytes,
        author: signature_to_wire(&commit.author),
        committer: signature_to_wire(&commit.committer),
        description: commit.description.clone(),
        secure_sig: commit.secure_sig.as_ref().map(secure_sig_to_wire),
        file_changes,
    }
}

fn commit_from_wire(wire: WireJjCommit) -> Commit {
    let root_tree_id = TreeId::from_bytes(&wire.root_tree);
    Commit {
        parents: wire.parents.into_iter().map(|b| CommitId::from_bytes(&b)).collect(),
        predecessors: vec![],
        root_tree: Merge::resolved(root_tree_id),
        conflict_labels: Merge::resolved(String::new()),
        change_id: ChangeId::from_bytes(&wire.change_id),
        description: wire.description,
        author: signature_from_wire(wire.author),
        committer: signature_from_wire(wire.committer),
        secure_sig: wire.secure_sig.map(secure_sig_from_wire),
    }
}

fn signature_to_wire(sig: &Signature) -> WireJjSignature {
    WireJjSignature {
        name: sig.name.clone(),
        email: sig.email.clone(),
        timestamp: sig.timestamp.timestamp.0,
        tz_offset: sig.timestamp.tz_offset,
    }
}

fn signature_from_wire(wire: WireJjSignature) -> Signature {
    Signature {
        name: wire.name,
        email: wire.email,
        timestamp: Timestamp {
            timestamp: jj_lib::backend::MillisSinceEpoch(wire.timestamp),
            tz_offset: wire.tz_offset,
        },
    }
}

fn secure_sig_to_wire(sig: &SecureSig) -> WireJjSecureSig {
    WireJjSecureSig {
        data: sig.data.clone(),
        sig: sig.sig.clone(),
    }
}

fn secure_sig_from_wire(wire: WireJjSecureSig) -> SecureSig {
    SecureSig {
        data: wire.data,
        sig: wire.sig,
    }
}

pub(crate) fn tree_to_wire(tree: &Tree) -> WireJjTree {
    let mut entries = std::collections::HashMap::new();
    for entry in tree.entries() {
        let name_str = entry.name().as_internal_str().to_string();
        let value = entry.value();
        let wire_entry = match value {
            TreeValue::File { id, executable, copy_id: _ } => {
                WireJjTreeEntry::File(WireJjTreeEntryFile {
                    file_id: id.as_bytes().to_vec(),
                    executable: *executable,
                })
            }
            TreeValue::Symlink(id) => {
                WireJjTreeEntry::Symlink(WireJjTreeEntrySymlink {
                    file_id: id.as_bytes().to_vec(),
                })
            }
            TreeValue::Tree(id) => {
                WireJjTreeEntry::Directory(WireJjTreeEntryDirectory {
                    tree_id: id.as_bytes().to_vec(),
                })
            }
            TreeValue::GitSubmodule(_) => {
                // No wire equivalent; map to Unknown.
                WireJjTreeEntry::Unknown
            }
        };
        entries.insert(name_str, wire_entry);
    }
    WireJjTree {
        tree_id: vec![],
        entries,
    }
}

fn tree_from_wire(wire: WireJjTree) -> BackendResult<Tree> {
    let mut entries: Vec<(RepoPathComponentBuf, TreeValue)> = Vec::new();
    for (name, wire_entry) in wire.entries {
        let component = RepoPathComponentBuf::new(name).map_err(|e| BackendError::Other(
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid tree entry name: {}", e.to_string()),
            )),
        ))?;
        let value = match wire_entry {
            WireJjTreeEntry::File(f) => TreeValue::File {
                id: FileId::from_bytes(&f.file_id),
                executable: f.executable,
                copy_id: CopyId::placeholder(),
            },
            WireJjTreeEntry::Symlink(s) => {
                TreeValue::Symlink(SymlinkId::from_bytes(&s.file_id))
            }
            WireJjTreeEntry::Directory(d) => {
                TreeValue::Tree(TreeId::from_bytes(&d.tree_id))
            }
            WireJjTreeEntry::Unknown => continue,
        };
        entries.push((component, value));
    }
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(Tree::from_sorted_entries(entries))
}
