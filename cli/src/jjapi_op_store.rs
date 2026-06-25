/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! HTTP-backed `OpStore` and `OpHeadsStore` delegating to `jj_edenapi::JjRemoteApi`.

use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use jj_edenapi::JjRemoteApi;

use once_cell::sync::Lazy;

static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Runtime::new().expect("failed to initialize the JJAPI tokio runtime")
});

use jj_lib::op_heads_store::OpHeadsStore;
use jj_lib::op_heads_store::OpHeadsStoreError;
use jj_lib::op_heads_store::OpHeadsStoreLock;
use jj_lib::op_store::OpStore;
use jj_lib::op_store::OpStoreError;
use jj_lib::op_store::OpStoreResult;
use jj_lib::op_store::Operation;
use jj_lib::op_store::OperationId;
use jj_lib::op_store::RootOperationData;
use jj_lib::op_store::View;
use jj_lib::op_store::ViewId;
use jj_lib::object_id::HexPrefix;
use jj_lib::object_id::ObjectId;
use jj_lib::object_id::PrefixResolution;
use jj_lib::ref_name::WorkspaceNameBuf;
use jj_lib::simple_op_store::decode_operation_from_proto_bytes;
use jj_lib::simple_op_store::decode_view_from_proto_bytes;
use jj_lib::simple_op_store::encode_operation_to_proto_bytes;
use jj_lib::simple_op_store::encode_view_to_proto_bytes;

use jj_edenapi_types::wire::opstore::WireCompareAndSwapOpHeadsRequest;
use jj_edenapi_types::wire::opstore::WireJjOpStorePayloadEncoding;
use jj_edenapi_types::wire::opstore::WireJjOperationObject;
use jj_edenapi_types::wire::opstore::WireJjViewObject;
use jj_edenapi_types::wire::opstore::WireReadOpHeadsRequest;
use jj_edenapi_types::wire::opstore::WireReadOperationRequest;
use jj_edenapi_types::wire::opstore::WireReadViewRequest;
use jj_edenapi_types::wire::opstore::WireWriteOperationRequest;
use jj_edenapi_types::wire::opstore::WireWriteViewRequest;
use jj_edenapi_types::wire::workspace::WirePutWorkspaceRequest;

fn map_api_error(err: jj_edenapi::JjRemoteApiError) -> OpStoreError {
    OpStoreError::Other(Box::new(err))
}

fn map_op_heads_error(err: jj_edenapi::JjRemoteApiError) -> OpHeadsStoreError {
    OpHeadsStoreError::Read(Box::new(err))
}

fn missing_object_err(kind: &str, id: &impl ObjectId) -> OpStoreError {
    OpStoreError::ObjectNotFound {
        object_type: kind.to_string(),
        hash: id.hex(),
        source: io::Error::new(io::ErrorKind::NotFound, "remote JJ opstore returned no object").into(),
    }
}

fn unsupported_encoding_err(kind: &str) -> OpStoreError {
    OpStoreError::ReadObject {
        object_type: kind.to_string(),
        hash: "<unknown>".to_string(),
        source: io::Error::new(io::ErrorKind::InvalidData, "unsupported remote JJ opstore payload encoding").into(),
    }
}

fn id_mismatch_err(kind: &str, expected: &impl ObjectId, actual: &impl ObjectId) -> OpStoreError {
    OpStoreError::ReadObject {
        object_type: kind.to_string(),
        hash: expected.hex(),
        source: io::Error::new(
            io::ErrorKind::InvalidData,
            format!("remote JJ opstore returned object id {}, expected {}", actual.hex(), expected.hex()),
        ).into(),
    }
}

/// HTTP-backed `OpStore` and `OpHeadsStore`.
pub struct JjapiOpStore {
    repo_name: String,
    workspace: WorkspaceNameBuf,
    root_data: RootOperationData,
    root_operation_id: OperationId,
    root_view_id: ViewId,
    client: Arc<dyn JjRemoteApi>,
}

impl fmt::Debug for JjapiOpStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JjapiOpStore")
            .field("repo_name", &self.repo_name)
            .field("workspace", &self.workspace)
            .finish_non_exhaustive()
    }
}

impl JjapiOpStore {
    pub fn new(
        repo_name: String,
        workspace: WorkspaceNameBuf,
        root_data: RootOperationData,
        root_operation_id: OperationId,
        root_view_id: ViewId,
        client: Arc<dyn JjRemoteApi>,
    ) -> Self {
        Self {
            repo_name,
            workspace,
            root_data,
            root_operation_id,
            root_view_id,
            client,
        }
    }

    pub fn name() -> &'static str {
        "jjapi-opstore"
    }

    pub fn op_heads_name() -> &'static str {
        "jjapi-opheads"
    }

    fn block_on<F: std::future::Future>(&self, f: F) -> F::Output {
        RUNTIME.block_on(f)
    }
}

#[async_trait]
impl OpStore for JjapiOpStore {
    fn name(&self) -> &str {
        Self::name()
    }

    fn root_operation_id(&self) -> &OperationId {
        &self.root_operation_id
    }

    async fn read_view(&self, id: &ViewId) -> OpStoreResult<View> {
        self.block_on(async {
            if *id == self.root_view_id {
                return Ok(View::make_root(self.root_data.root_commit_id.clone()));
            }
            let req = WireReadViewRequest {
                repo: self.repo_name.clone(),
                view_id: id.as_bytes().to_vec(),
            };
            let resp = self.client.read_view(req).await.map_err(map_api_error)?;
            let entries = resp.flatten().await.map_err(map_api_error)?;
            let entry = entries.into_iter().next().ok_or_else(|| missing_object_err("view", id))?;
            let object = entry.view.ok_or_else(|| missing_object_err("view", id))?;
            if object.view_id != id.as_bytes().to_vec() {
                return Err(id_mismatch_err("view", id, &ViewId::from_bytes(&object.view_id)));
            }
            if object.encoding != WireJjOpStorePayloadEncoding::SimpleOpStoreProtoV1 {
                return Err(unsupported_encoding_err("view"));
            }
            decode_view_from_proto_bytes(id, &object.payload)
        })
    }

    async fn write_view(&self, contents: &View) -> OpStoreResult<ViewId> {
        self.block_on(async {
            let (view_id, payload) = encode_view_to_proto_bytes(contents);
            let req = WireWriteViewRequest {
                repo: self.repo_name.clone(),
                view: WireJjViewObject {
                    view_id: view_id.as_bytes().to_vec(),
                    encoding: WireJjOpStorePayloadEncoding::SimpleOpStoreProtoV1,
                    payload,
                },
            };
            let resp = self.client.write_view(req).await.map_err(map_api_error)?;
            let entries = resp.flatten().await.map_err(map_api_error)?;
            let _ = entries.into_iter().next().ok_or_else(|| OpStoreError::WriteObject {
                object_type: "view",
                source: io::Error::new(io::ErrorKind::UnexpectedEof, "empty response").into(),
            })?;
            Ok(view_id)
        })
    }

    async fn read_operation(&self, id: &OperationId) -> OpStoreResult<Operation> {
        self.block_on(async {
            if *id == self.root_operation_id {
                return Ok(Operation::make_root(self.root_view_id.clone()));
            }
            let req = WireReadOperationRequest {
                repo: self.repo_name.clone(),
                operation_id: id.as_bytes().to_vec(),
            };
            let resp = self.client.read_operation(req).await.map_err(map_api_error)?;
            let entries = resp.flatten().await.map_err(map_api_error)?;
            let entry = entries.into_iter().next().ok_or_else(|| missing_object_err("operation", id))?;
            let object = entry.operation.ok_or_else(|| missing_object_err("operation", id))?;
            if object.operation_id != id.as_bytes().to_vec() {
                return Err(id_mismatch_err("operation", id, &OperationId::from_bytes(&object.operation_id)));
            }
            if object.encoding != WireJjOpStorePayloadEncoding::SimpleOpStoreProtoV1 {
                return Err(unsupported_encoding_err("operation"));
            }
            decode_operation_from_proto_bytes(id, &object.payload)
        })
    }

    async fn write_operation(&self, contents: &Operation) -> OpStoreResult<OperationId> {
        self.block_on(async {
            let (operation_id, payload) = encode_operation_to_proto_bytes(contents)?;
            let req = WireWriteOperationRequest {
                repo: self.repo_name.clone(),
                operation: WireJjOperationObject {
                    operation_id: operation_id.as_bytes().to_vec(),
                    encoding: WireJjOpStorePayloadEncoding::SimpleOpStoreProtoV1,
                    payload,
                },
            };
            let resp = self.client.write_operation(req).await.map_err(map_api_error)?;
            let entries = resp.flatten().await.map_err(map_api_error)?;
            let _ = entries.into_iter().next().ok_or_else(|| OpStoreError::WriteObject {
                object_type: "operation",
                source: io::Error::new(io::ErrorKind::UnexpectedEof, "empty response").into(),
            })?;
            Ok(operation_id)
        })
    }

    async fn resolve_operation_id_prefix(
        &self,
        _prefix: &HexPrefix,
    ) -> OpStoreResult<PrefixResolution<OperationId>> {
        self.block_on(async {
            // Not implemented on the HTTP surface yet.
            Ok(PrefixResolution::NoMatch)
        })
    }

    async fn gc(&self, _head_ids: &[OperationId], _keep_newer: SystemTime) -> OpStoreResult<()> {
        self.block_on(async {
            Ok(())
        })
    }
}

struct JjapiOpHeadsStoreLock;

impl OpHeadsStoreLock for JjapiOpHeadsStoreLock {}

#[async_trait]
impl OpHeadsStore for JjapiOpStore {
    fn name(&self) -> &str {
        Self::op_heads_name()
    }

    async fn update_op_heads(
        &self,
        old_ids: &[OperationId],
        new_id: &OperationId,
    ) -> Result<(), OpHeadsStoreError> {
        self.block_on(async {
            let req = WireCompareAndSwapOpHeadsRequest {
                repo: self.repo_name.clone(),
                workspace: self.workspace.as_str().to_string(),
                expected_operation_ids: old_ids.iter().map(|id| id.as_bytes().to_vec()).collect(),
                new_operation_ids: vec![new_id.as_bytes().to_vec()],
                transaction_id: None,
            };
            let resp = self.client.compare_and_swap_op_heads(req).await.map_err(|e| OpHeadsStoreError::Write {
                new_op_id: new_id.clone(),
                source: Box::new(e),
            })?;
            let _ = resp.flatten().await.map_err(|e| OpHeadsStoreError::Write {
                new_op_id: new_id.clone(),
                source: Box::new(e),
            })?;
            let operation = if *new_id == self.root_operation_id {
                Operation::make_root(self.root_view_id.clone())
            } else {
                let req = WireReadOperationRequest {
                    repo: self.repo_name.clone(),
                    operation_id: new_id.as_bytes().to_vec(),
                };
                let resp = self.client.read_operation(req).await.map_err(|e| {
                    OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(e),
                    }
                })?;
                let entries = resp.flatten().await.map_err(|e| OpHeadsStoreError::Write {
                    new_op_id: new_id.clone(),
                    source: Box::new(e),
                })?;
                let entry = entries.into_iter().next().ok_or_else(|| {
                    OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "empty operation response after op-head CAS",
                        )),
                    }
                })?;
                let object = entry.operation.ok_or_else(|| OpHeadsStoreError::Write {
                    new_op_id: new_id.clone(),
                    source: Box::new(io::Error::new(
                        io::ErrorKind::NotFound,
                        "operation not found after op-head CAS",
                    )),
                })?;
                if object.operation_id != new_id.as_bytes().to_vec() {
                    return Err(OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "operation id mismatch after op-head CAS",
                        )),
                    });
                }
                if object.encoding != WireJjOpStorePayloadEncoding::SimpleOpStoreProtoV1 {
                    return Err(OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unsupported operation encoding after op-head CAS",
                        )),
                    });
                }
                decode_operation_from_proto_bytes(new_id, &object.payload).map_err(|e| {
                    OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(e),
                    }
                })?
            };
            let view = if operation.view_id == self.root_view_id {
                View::make_root(self.root_data.root_commit_id.clone())
            } else {
                let req = WireReadViewRequest {
                    repo: self.repo_name.clone(),
                    view_id: operation.view_id.as_bytes().to_vec(),
                };
                let resp = self.client.read_view(req).await.map_err(|e| {
                    OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(e),
                    }
                })?;
                let entries = resp.flatten().await.map_err(|e| OpHeadsStoreError::Write {
                    new_op_id: new_id.clone(),
                    source: Box::new(e),
                })?;
                let entry = entries.into_iter().next().ok_or_else(|| {
                    OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "empty view response after op-head CAS",
                        )),
                    }
                })?;
                let object = entry.view.ok_or_else(|| OpHeadsStoreError::Write {
                    new_op_id: new_id.clone(),
                    source: Box::new(io::Error::new(
                        io::ErrorKind::NotFound,
                        "view not found after op-head CAS",
                    )),
                })?;
                if object.view_id != operation.view_id.as_bytes().to_vec() {
                    return Err(OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "view id mismatch after op-head CAS",
                        )),
                    });
                }
                if object.encoding != WireJjOpStorePayloadEncoding::SimpleOpStoreProtoV1 {
                    return Err(OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unsupported view encoding after op-head CAS",
                        )),
                    });
                }
                decode_view_from_proto_bytes(&operation.view_id, &object.payload).map_err(|e| {
                    OpHeadsStoreError::Write {
                        new_op_id: new_id.clone(),
                        source: Box::new(e),
                    }
                })?
            };
            let working_copy_parent = view
                .wc_commit_ids
                .get(&self.workspace)
                .map(|id| id.as_bytes().to_vec());
            let req = WirePutWorkspaceRequest {
                repo: self.repo_name.clone(),
                workspace: self.workspace.as_str().to_string(),
                current_view_id: operation.view_id.as_bytes().to_vec(),
                current_operation_id: new_id.as_bytes().to_vec(),
                working_copy_parent,
                expected_current_operation_id: None,
            };
            let resp = self.client.put_workspace(req).await.map_err(|e| {
                OpHeadsStoreError::Write {
                    new_op_id: new_id.clone(),
                    source: Box::new(e),
                }
            })?;
            let _ = resp.flatten().await.map_err(|e| OpHeadsStoreError::Write {
                new_op_id: new_id.clone(),
                source: Box::new(e),
            })?;
            Ok(())
        })
    }

    async fn get_op_heads(&self) -> Result<Vec<OperationId>, OpHeadsStoreError> {
        self.block_on(async {
            let req = WireReadOpHeadsRequest {
                repo: self.repo_name.clone(),
                workspace: self.workspace.as_str().to_string(),
            };
            let resp = self.client.read_op_heads(req).await.map_err(map_op_heads_error)?;
            let entries = resp.flatten().await.map_err(map_op_heads_error)?;
            let entry = entries.into_iter().next().unwrap_or_default();
            let mut ids: Vec<OperationId> = entry.state.operation_ids.into_iter().map(|b| OperationId::from_bytes(&b)).collect();
            ids.sort();
            ids.dedup();
            Ok(ids)
        })
    }

    async fn lock(&self) -> Result<Box<dyn OpHeadsStoreLock + '_>, OpHeadsStoreError> {
        self.block_on(async {
            let ret: Box<dyn OpHeadsStoreLock> = Box::new(JjapiOpHeadsStoreLock);
            Ok(ret)
        })
    }
}
