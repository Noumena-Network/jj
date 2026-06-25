/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! EdenFS-backed working-copy adapter for JJ-on-Mononoke.
//!
//! In EdenFS mode the virtual filesystem layer manages the on-disk working-copy
//! state. JJ only tracks the logical checkout position (operation id and tree
//! id).  Snapshot and checkout delegate to EdenFS in the long term; the
//! current implementation stores the minimal JJ state locally and documents the
//! planned EdenFS API calls.

#![expect(missing_docs)]

use std::fs::File;
use std::io;
use std::io::Read as _;
use std::io::Write as _;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt as _;
use futures::io::AllowStdIo;
use futures::stream;
use once_cell::unsync::OnceCell;
use tracing::instrument;

use crate::backend::CopyId;
use crate::backend::TreeValue;
use crate::commit::Commit;
use crate::conflicts::MaterializedTreeValue;
use crate::conflicts::materialize_tree_value;
use crate::lock::FileLock;
use crate::matchers::EverythingMatcher;
use crate::merge::Merge;
use crate::merged_tree::MergedTree;
use crate::merged_tree::TreeDiffEntry;
use crate::merged_tree_builder::MergedTreeBuilder;
use crate::object_id::ObjectId as _;
use crate::op_store::OperationId;
use crate::ref_name::WorkspaceName;
use crate::ref_name::WorkspaceNameBuf;
use crate::repo_path::RepoPath;
use crate::repo_path::RepoPathBuf;
use crate::settings::UserSettings;
use crate::store::Store;
use crate::working_copy::CheckoutError;
use crate::working_copy::CheckoutStats;
use crate::working_copy::LockedWorkingCopy;
use crate::working_copy::ResetError;
use crate::working_copy::SnapshotError;
use crate::working_copy::SnapshotOptions;
use crate::working_copy::SnapshotStats;
use crate::working_copy::WorkingCopy;
use crate::working_copy::WorkingCopyFactory;
use crate::working_copy::WorkingCopyStateError;

// ---------------------------------------------------------------------------
// On-disk state format
// ---------------------------------------------------------------------------

/// Compact binary state file for the EdenFS working copy.
///
/// Version 2 layout (little-endian lengths):
///   magic:          [u8; 4] = b"EJFS"
///   version:        u32 = 2
///   op_id_len:      u64
///   op_id:          [u8; op_id_len]
///   ws_len:         u64
///   workspace:      utf8 bytes
///   tree_id_len:    u64
///   tree_id:        [u8; tree_id_len]
///   sparse_count:   u64
///   for each sparse pattern:
///     pat_len:      u64
///     pat:          utf8 bytes
///   has_journal_position: u8
///   if has_journal_position == 1:
///     mount_generation: i64
///     sequence_number:  u64
///     snapshot_hash_len: u64
///     snapshot_hash:     [u8; snapshot_hash_len]
const STATE_MAGIC: &[u8; 4] = b"EJFS";
const STATE_VERSION: u32 = 2;
const STATE_FILENAME: &str = "eden_fs_checkout";
const LOCK_FILENAME: &str = "eden_fs_working_copy.lock";

#[derive(Clone, Debug, PartialEq, Eq)]
struct EdenFsJournalPosition {
    mount_generation: i64,
    sequence_number: u64,
    snapshot_hash: Vec<u8>,
}

#[cfg(fbcode_build)]
impl From<&EdenFsJournalPosition> for edenfs_client::types::JournalPosition {
    fn from(position: &EdenFsJournalPosition) -> Self {
        Self {
            mount_generation: position.mount_generation,
            sequence_number: position.sequence_number,
            snapshot_hash: position.snapshot_hash.clone(),
        }
    }
}

#[cfg(fbcode_build)]
impl From<edenfs_client::types::JournalPosition> for EdenFsJournalPosition {
    fn from(position: edenfs_client::types::JournalPosition) -> Self {
        Self {
            mount_generation: position.mount_generation,
            sequence_number: position.sequence_number,
            snapshot_hash: position.snapshot_hash,
        }
    }
}

#[cfg(fbcode_build)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EdenFsChangedPathKind {
    Upsert,
    Delete,
}

#[cfg(fbcode_build)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct EdenFsChangedPath {
    path: RepoPathBuf,
    kind: EdenFsChangedPathKind,
}

#[cfg(fbcode_build)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct EdenFsCollectedChanges {
    to_position: EdenFsJournalPosition,
    changed_paths: Vec<EdenFsChangedPath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EdenFsCheckoutState {
    operation_id: OperationId,
    workspace_name: WorkspaceNameBuf,
    tree_id: crate::backend::TreeId,
    sparse_patterns: Vec<RepoPathBuf>,
    journal_position: Option<EdenFsJournalPosition>,
}

impl EdenFsCheckoutState {
    fn save(&self, state_path: &Path) -> io::Result<()> {
        let path = state_path.join(STATE_FILENAME);
        let mut buf = Vec::new();
        buf.write_all(STATE_MAGIC)?;
        buf.write_all(&STATE_VERSION.to_le_bytes())?;

        let op_id = self.operation_id.to_bytes();
        buf.write_all(&(op_id.len() as u64).to_le_bytes())?;
        buf.write_all(&op_id)?;

        let ws = self.workspace_name.as_str().as_bytes();
        buf.write_all(&(ws.len() as u64).to_le_bytes())?;
        buf.write_all(ws)?;

        let tree_id = self.tree_id.to_bytes();
        buf.write_all(&(tree_id.len() as u64).to_le_bytes())?;
        buf.write_all(&tree_id)?;

        buf.write_all(&(self.sparse_patterns.len() as u64).to_le_bytes())?;
        for pat in &self.sparse_patterns {
            let p = pat.as_internal_file_string().as_bytes();
            buf.write_all(&(p.len() as u64).to_le_bytes())?;
            buf.write_all(p)?;
        }

        match &self.journal_position {
            Some(position) => {
                buf.write_all(&[1])?;
                buf.write_all(&position.mount_generation.to_le_bytes())?;
                buf.write_all(&position.sequence_number.to_le_bytes())?;
                buf.write_all(&(position.snapshot_hash.len() as u64).to_le_bytes())?;
                buf.write_all(&position.snapshot_hash)?;
            }
            None => {
                buf.write_all(&[0])?;
            }
        }

        let mut tmp = tempfile::NamedTempFile::new_in(state_path)?;
        tmp.write_all(&buf)?;
        tmp.persist(path)?;
        Ok(())
    }

    fn load(state_path: &Path) -> io::Result<Self> {
        let path = state_path.join(STATE_FILENAME);
        let mut file = File::open(&path)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut cursor = io::Cursor::new(&buf);

        let mut magic = [0u8; 4];
        cursor.read_exact(&mut magic)?;
        if &magic != STATE_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad EdenFS state magic",
            ));
        }

        let mut version = [0u8; 4];
        cursor.read_exact(&mut version)?;
        let version = u32::from_le_bytes(version);
        if version != 1 && version != STATE_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported EdenFS state version {version}"),
            ));
        }

        let read_bytes = |cursor: &mut io::Cursor<&Vec<u8>>| -> io::Result<Vec<u8>> {
            let mut len = [0u8; 8];
            cursor.read_exact(&mut len)?;
            let len = u64::from_le_bytes(len) as usize;
            let mut buf = vec![0u8; len];
            cursor.read_exact(&mut buf)?;
            Ok(buf)
        };

        let operation_id = OperationId::new(read_bytes(&mut cursor)?);
        let workspace_name = String::from_utf8(read_bytes(&mut cursor)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .into();
        let tree_id = crate::backend::TreeId::new(read_bytes(&mut cursor)?);

        let mut sparse_count = [0u8; 8];
        cursor.read_exact(&mut sparse_count)?;
        let sparse_count = u64::from_le_bytes(sparse_count) as usize;
        let mut sparse_patterns = Vec::with_capacity(sparse_count);
        for _ in 0..sparse_count {
            let pat = String::from_utf8(read_bytes(&mut cursor)?)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            sparse_patterns.push(
                RepoPath::from_internal_string(&pat)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
                    .to_owned(),
            );
        }
        let journal_position = if version >= 2 {
            let mut has_journal_position = [0u8; 1];
            cursor.read_exact(&mut has_journal_position)?;
            match has_journal_position[0] {
                0 => None,
                1 => {
                    let mut mount_generation = [0u8; 8];
                    cursor.read_exact(&mut mount_generation)?;
                    let mut sequence_number = [0u8; 8];
                    cursor.read_exact(&mut sequence_number)?;
                    Some(EdenFsJournalPosition {
                        mount_generation: i64::from_le_bytes(mount_generation),
                        sequence_number: u64::from_le_bytes(sequence_number),
                        snapshot_hash: read_bytes(&mut cursor)?,
                    })
                }
                value => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid journal-position presence flag {value}"),
                    ));
                }
            }
        } else {
            None
        };

        Ok(Self {
            operation_id,
            workspace_name,
            tree_id,
            sparse_patterns,
            journal_position,
        })
    }
}

#[cfg(fbcode_build)]
fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(fbcode_build)]
fn edenfs_snapshot_error(message: impl Into<String>) -> SnapshotError {
    SnapshotError::Other {
        message: message.into(),
        err: Box::new(io::Error::other("JJ EdenFS snapshot collector failed")),
    }
}

#[cfg(fbcode_build)]
fn repo_path_from_eden_bytes(path: &[u8]) -> Result<RepoPathBuf, SnapshotError> {
    let path = std::str::from_utf8(path).map_err(|_| SnapshotError::InvalidUtf8Path {
        path: String::from_utf8_lossy(path).into_owned().into(),
    })?;
    RepoPathBuf::from_internal_string(path)
        .map_err(|err| edenfs_snapshot_error(format!("invalid EdenFS change path {path:?}: {err}")))
}

#[cfg(fbcode_build)]
fn repo_path_from_relative_fs_path(path: &Path) -> Result<RepoPathBuf, SnapshotError> {
    RepoPathBuf::from_relative_path(path).map_err(|err| {
        edenfs_snapshot_error(format!(
            "invalid EdenFS relative filesystem path {}: {err}",
            path.display()
        ))
    })
}

#[cfg(fbcode_build)]
fn is_jj_internal_path(path: &RepoPath) -> bool {
    [".jj", ".hg", ".eden"]
        .into_iter()
        .any(|prefix| path.starts_with(RepoPath::from_internal_string(prefix).unwrap()))
}

#[cfg(fbcode_build)]
fn collect_existing_edenfs_paths(mount_path: &Path) -> Result<Vec<EdenFsChangedPath>, SnapshotError> {
    fn visit(
        root: &Path,
        current: &Path,
        out: &mut Vec<EdenFsChangedPath>,
    ) -> Result<(), SnapshotError> {
        for entry in std::fs::read_dir(current).map_err(|err| SnapshotError::Other {
            message: format!("Failed to read directory {}", current.display()),
            err: err.into(),
        })? {
            let entry = entry.map_err(|err| SnapshotError::Other {
                message: format!("Failed to read directory entry in {}", current.display()),
                err: err.into(),
            })?;
            let path = entry.path();
            let relative = path.strip_prefix(root).map_err(|err| {
                edenfs_snapshot_error(format!(
                    "failed to relativize {} against {}: {err}",
                    path.display(),
                    root.display()
                ))
            })?;
            let repo_path = repo_path_from_relative_fs_path(relative)?;
            if is_jj_internal_path(repo_path.as_ref()) {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path).map_err(|err| {
                SnapshotError::Other {
                    message: format!("Failed to stat {}", path.display()),
                    err: err.into(),
                }
            })?;
            let file_type = metadata.file_type();
            if file_type.is_dir() {
                visit(root, &path, out)?;
            } else if file_type.is_file() || file_type.is_symlink() {
                out.push(EdenFsChangedPath {
                    path: repo_path,
                    kind: EdenFsChangedPathKind::Upsert,
                });
            } else {
                return Err(edenfs_snapshot_error(format!(
                    "unsupported existing EdenFS file type at {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    let mut paths = Vec::new();
    visit(mount_path, mount_path, &mut paths)?;
    paths.sort_by(|a, b| a.path.as_internal_file_string().cmp(b.path.as_internal_file_string()));
    paths.dedup();
    Ok(paths)
}

#[cfg(fbcode_build)]
fn sparse_contains(path: &RepoPath, sparse_patterns: &[RepoPathBuf]) -> bool {
    sparse_patterns
        .iter()
        .any(|pattern| pattern.is_root() || path.starts_with(pattern.as_ref()))
}

#[cfg(fbcode_build)]
fn push_upsert_change(
    changes: &mut Vec<EdenFsChangedPath>,
    path: &[u8],
    dtype: edenfs_client::types::Dtype,
) -> Result<(), SnapshotError> {
    match dtype {
        edenfs_client::types::Dtype::Regular | edenfs_client::types::Dtype::Link => {
            let path = repo_path_from_eden_bytes(path)?;
            if is_jj_internal_path(path.as_ref()) {
                return Ok(());
            }
            changes.push(EdenFsChangedPath {
                path,
                kind: EdenFsChangedPathKind::Upsert,
            });
            Ok(())
        }
        edenfs_client::types::Dtype::Dir => Ok(()),
        dtype => Err(edenfs_snapshot_error(format!(
            "unsupported EdenFS upsert file type {dtype} for path {}",
            String::from_utf8_lossy(path)
        ))),
    }
}

#[cfg(fbcode_build)]
fn push_delete_change(
    changes: &mut Vec<EdenFsChangedPath>,
    path: &[u8],
) -> Result<(), SnapshotError> {
    let path = repo_path_from_eden_bytes(path)?;
    if is_jj_internal_path(path.as_ref()) {
        return Ok(());
    }
    changes.push(EdenFsChangedPath {
        path,
        kind: EdenFsChangedPathKind::Delete,
    });
    Ok(())
}

#[cfg(fbcode_build)]
fn classify_edenfs_changes(
    result: edenfs_client::changes_since::ChangesSinceV2Result,
) -> Result<EdenFsCollectedChanges, SnapshotError> {
    use edenfs_client::changes_since::ChangeNotification;
    use edenfs_client::changes_since::LargeChangeNotification;
    use edenfs_client::changes_since::SmallChangeNotification;
    use edenfs_client::changes_since::StateChangeNotification;

    let mut changed_paths = Vec::new();
    for change in result.changes {
        match change {
            ChangeNotification::SmallChange(SmallChangeNotification::Added(change)) => {
                push_upsert_change(&mut changed_paths, &change.path, change.file_type)?;
            }
            ChangeNotification::SmallChange(SmallChangeNotification::Modified(change)) => {
                push_upsert_change(&mut changed_paths, &change.path, change.file_type)?;
            }
            ChangeNotification::SmallChange(SmallChangeNotification::Removed(change)) => {
                push_delete_change(&mut changed_paths, &change.path)?;
            }
            ChangeNotification::SmallChange(SmallChangeNotification::Renamed(change)) => {
                if change.file_type == edenfs_client::types::Dtype::Dir {
                    return Err(edenfs_snapshot_error(format!(
                        "directory rename from {} to {} requires recursive EdenFS change expansion",
                        String::from_utf8_lossy(&change.from),
                        String::from_utf8_lossy(&change.to)
                    )));
                }
                push_delete_change(&mut changed_paths, &change.from)?;
                push_upsert_change(&mut changed_paths, &change.to, change.file_type)?;
            }
            ChangeNotification::SmallChange(SmallChangeNotification::Replaced(change)) => {
                if change.file_type == edenfs_client::types::Dtype::Dir {
                    return Err(edenfs_snapshot_error(format!(
                        "directory replacement from {} to {} requires recursive EdenFS change expansion",
                        String::from_utf8_lossy(&change.from),
                        String::from_utf8_lossy(&change.to)
                    )));
                }
                push_delete_change(&mut changed_paths, &change.from)?;
                push_upsert_change(&mut changed_paths, &change.to, change.file_type)?;
            }
            ChangeNotification::SmallChange(SmallChangeNotification::UnknownField(field)) => {
                return Err(edenfs_snapshot_error(format!(
                    "unknown EdenFS small change field {field}"
                )));
            }
            ChangeNotification::LargeChange(LargeChangeNotification::DirectoryRenamed(change)) => {
                return Err(edenfs_snapshot_error(format!(
                    "large directory rename from {} to {} requires recursive EdenFS change expansion",
                    String::from_utf8_lossy(&change.from),
                    String::from_utf8_lossy(&change.to)
                )));
            }
            ChangeNotification::LargeChange(LargeChangeNotification::CommitTransition(change)) => {
                return Err(edenfs_snapshot_error(format!(
                    "EdenFS commit transition {} -> {} requires checkout-aware snapshot recovery",
                    bytes_to_hex(&change.from),
                    bytes_to_hex(&change.to)
                )));
            }
            ChangeNotification::LargeChange(LargeChangeNotification::LostChanges(change)) => {
                return Err(edenfs_snapshot_error(format!(
                    "EdenFS journal lost changes: {}",
                    change.reason
                )));
            }
            ChangeNotification::LargeChange(LargeChangeNotification::UnknownField(field)) => {
                return Err(edenfs_snapshot_error(format!(
                    "unknown EdenFS large change field {field}"
                )));
            }
            ChangeNotification::StateChange(StateChangeNotification::StateEntered(_))
            | ChangeNotification::StateChange(StateChangeNotification::StateLeft(_)) => {}
            ChangeNotification::StateChange(StateChangeNotification::UnknownField(field)) => {
                return Err(edenfs_snapshot_error(format!(
                    "unknown EdenFS state change field {field}"
                )));
            }
            ChangeNotification::UnknownField(field) => {
                return Err(edenfs_snapshot_error(format!(
                    "unknown EdenFS change field {field}"
                )));
            }
        }
    }

    changed_paths.sort_by(|a, b| {
        a.path
            .as_internal_file_string()
            .cmp(b.path.as_internal_file_string())
            .then_with(|| match (a.kind, b.kind) {
                (EdenFsChangedPathKind::Delete, EdenFsChangedPathKind::Upsert) => {
                    std::cmp::Ordering::Less
                }
                (EdenFsChangedPathKind::Upsert, EdenFsChangedPathKind::Delete) => {
                    std::cmp::Ordering::Greater
                }
                _ => std::cmp::Ordering::Equal,
            })
    });
    changed_paths.dedup();

    Ok(EdenFsCollectedChanges {
        to_position: result.to_position.into(),
        changed_paths,
    })
}

#[cfg(fbcode_build)]
async fn collect_edenfs_changes_since(
    client: &edenfs_client::client::EdenFsClient,
    mount_point: &Path,
    from_position: &EdenFsJournalPosition,
) -> Result<EdenFsCollectedChanges, SnapshotError> {
    let from_position = from_position.into();
    let result = client
        .get_changes_since(
            &Some(mount_point.to_path_buf()),
            &from_position,
            &None,
            &None,
            &None,
            &Some(vec![
                PathBuf::from(".jj"),
                PathBuf::from(".hg"),
                PathBuf::from(".eden"),
            ]),
            &None,
            false,
            true,
        )
        .await
        .map_err(|err| {
            edenfs_snapshot_error(format!(
                "failed to read EdenFS changesSinceV2 for {}: {err}",
                mount_point.display()
            ))
        })?;
    classify_edenfs_changes(result)
}

#[cfg(fbcode_build)]
async fn collect_edenfs_changes_for_mount(
    mount_point: PathBuf,
    from_position: EdenFsJournalPosition,
) -> Result<EdenFsCollectedChanges, SnapshotError> {
    async fn collect(
        mount_point: PathBuf,
        from_position: EdenFsJournalPosition,
    ) -> Result<EdenFsCollectedChanges, SnapshotError> {
        let instance = edenfs_instance_for_mount(&mount_point)?;
        let client = instance.get_client();
        collect_edenfs_changes_since(client.as_ref(), &mount_point, &from_position).await
    }

    if tokio::runtime::Handle::try_current().is_ok() {
        collect(mount_point, from_position).await
    } else {
        tokio::runtime::Runtime::new()
            .map_err(|err| edenfs_snapshot_error(format!("failed to create Tokio runtime for EdenFS changesSinceV2: {err}")))?
            .block_on(collect(mount_point, from_position))
    }
}

#[cfg(fbcode_build)]
async fn apply_edenfs_changes_to_tree(
    store: Arc<Store>,
    working_copy_path: PathBuf,
    base_tree: MergedTree,
    changes: &[EdenFsChangedPath],
) -> Result<MergedTree, SnapshotError> {
    let mut builder = MergedTreeBuilder::new(base_tree);
    let mut upserts = Vec::new();
    for change in changes {
        match change.kind {
            EdenFsChangedPathKind::Delete => {
                builder.set_or_remove(change.path.clone(), Merge::absent());
            }
            EdenFsChangedPathKind::Upsert => upserts.push(change.path.clone()),
        }
    }
    let concurrency = store.concurrency().max(1);
    let mut upsert_stream = stream::iter(upserts.into_iter().map(|path| {
        let store = store.clone();
        let working_copy_path = working_copy_path.clone();
        async move {
            let value = write_edenfs_path_to_store(store, &working_copy_path, &path).await?;
            Ok::<_, SnapshotError>((path, value))
        }
    }))
    .buffered(concurrency);
    while let Some(result) = upsert_stream.next().await {
        let (path, value) = result?;
        builder.set_or_remove(path, Merge::normal(value));
    }
    Ok(builder.write_tree().await?)
}

#[cfg(fbcode_build)]
async fn write_edenfs_path_to_store(
    store: Arc<Store>,
    working_copy_path: &Path,
    repo_path: &RepoPathBuf,
) -> Result<TreeValue, SnapshotError> {
    let disk_path = repo_path.to_fs_path(working_copy_path)?;
    let metadata = std::fs::symlink_metadata(&disk_path).map_err(|err| SnapshotError::Other {
        message: format!("Failed to stat EdenFS path {}", disk_path.display()),
        err: err.into(),
    })?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        let target = disk_path.read_link().map_err(|err| SnapshotError::Other {
            message: format!("Failed to read symlink {}", disk_path.display()),
            err: err.into(),
        })?;
        let target = target.into_os_string().into_string().map_err(|_| {
            SnapshotError::InvalidUtf8SymlinkTarget {
                path: disk_path.clone(),
            }
        })?;
        let id = store.write_symlink(repo_path, &target).await?;
        return Ok(TreeValue::Symlink(id));
    }

    if !file_type.is_file() {
        return Err(SnapshotError::Other {
            message: format!(
                "EdenFS changed path {} is not a regular file or symlink",
                disk_path.display()
            ),
            err: io::Error::new(io::ErrorKind::Unsupported, "unsupported file type").into(),
        });
    }

    let executable = {
        #[cfg(unix)]
        {
            metadata.permissions().mode() & 0o111 != 0
        }
        #[cfg(not(unix))]
        {
            false
        }
    };
    let file = File::open(&disk_path).map_err(|err| SnapshotError::Other {
        message: format!("Failed to open file {}", disk_path.display()),
        err: err.into(),
    })?;
    let mut contents = AllowStdIo::new(file);
    let id = store.write_file(repo_path, &mut contents).await?;
    Ok(TreeValue::File {
        id,
        executable,
        copy_id: CopyId::placeholder(),
    })
}

#[cfg(fbcode_build)]
async fn materialize_tree_to_edenfs_mount(
    store: Arc<Store>,
    working_copy_path: PathBuf,
    old_tree: MergedTree,
    new_tree: MergedTree,
    sparse_patterns: &[RepoPathBuf],
) -> Result<CheckoutStats, CheckoutError> {
    let mut stats = CheckoutStats::default();
    let matcher = EverythingMatcher;
    let mut diff_stream = old_tree.diff_stream_for_file_system(&new_tree, &matcher);

    while let Some(TreeDiffEntry { path, values }) = diff_stream.next().await {
        let diff = values?;
        let disk_path = path.to_fs_path(&working_copy_path)?;
        if diff.after.is_absent() || !sparse_contains(path.as_ref(), sparse_patterns) {
            remove_edenfs_path(&disk_path)?;
            stats.removed_files += 1;
            continue;
        }

        let materialized = materialize_tree_value(&store, &path, diff.after, new_tree.labels())
            .await?;
        write_materialized_value_to_edenfs_path(&disk_path, &path, materialized).await?;
        if diff.before.is_absent() {
            stats.added_files += 1;
        } else {
            stats.updated_files += 1;
        }
    }

    Ok(stats)
}

#[cfg(fbcode_build)]
fn remove_edenfs_path(path: &Path) -> Result<(), CheckoutError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(CheckoutError::Other {
                message: format!("Failed to stat path before removal {}", path.display()),
                err: err.into(),
            });
        }
    };
    let result = if metadata.file_type().is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|err| CheckoutError::Other {
        message: format!("Failed to remove path {}", path.display()),
        err: err.into(),
    })
}

#[cfg(fbcode_build)]
async fn write_materialized_value_to_edenfs_path(
    disk_path: &Path,
    repo_path: &RepoPath,
    value: MaterializedTreeValue,
) -> Result<(), CheckoutError> {
    if let Some(parent) = disk_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| CheckoutError::Other {
            message: format!("Failed to create parent directory {}", parent.display()),
            err: err.into(),
        })?;
    }

    match value {
        MaterializedTreeValue::File(mut file) => {
            remove_edenfs_path(disk_path)?;
            let contents = file.read_all(repo_path).await?;
            let mut output = File::create(disk_path).map_err(|err| CheckoutError::Other {
                message: format!("Failed to create file {}", disk_path.display()),
                err: err.into(),
            })?;
            output.write_all(&contents).map_err(|err| CheckoutError::Other {
                message: format!("Failed to write file {}", disk_path.display()),
                err: err.into(),
            })?;
            #[cfg(unix)]
            {
                let mode = if file.executable { 0o755 } else { 0o644 };
                let permissions = std::fs::Permissions::from_mode(mode);
                std::fs::set_permissions(disk_path, permissions).map_err(|err| {
                    CheckoutError::Other {
                        message: format!("Failed to set permissions on {}", disk_path.display()),
                        err: err.into(),
                    }
                })?;
            }
            Ok(())
        }
        MaterializedTreeValue::Symlink { target, .. } => {
            remove_edenfs_path(disk_path)?;
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&target, disk_path).map_err(|err| {
                    CheckoutError::Other {
                        message: format!("Failed to create symlink {}", disk_path.display()),
                        err: err.into(),
                    }
                })
            }
            #[cfg(not(unix))]
            {
                let _ = target;
                Err(CheckoutError::Other {
                    message: format!(
                        "EdenFS JJ checkout cannot materialize symlink {} on this platform yet",
                        disk_path.display()
                    ),
                    err: io::Error::new(io::ErrorKind::Unsupported, "symlink checkout").into(),
                })
            }
        }
        MaterializedTreeValue::Absent => {
            remove_edenfs_path(disk_path)?;
            Ok(())
        }
        MaterializedTreeValue::Tree(_)
        | MaterializedTreeValue::GitSubmodule(_)
        | MaterializedTreeValue::FileConflict(_)
        | MaterializedTreeValue::OtherConflict { .. }
        | MaterializedTreeValue::AccessDenied(_) => Err(CheckoutError::Other {
            message: format!(
                "EdenFS JJ checkout cannot materialize unsupported tree value at {} yet",
                disk_path.display()
            ),
            err: io::Error::new(io::ErrorKind::Unsupported, "unsupported tree value").into(),
        }),
    }
}

#[cfg(fbcode_build)]
fn edenfs_instance_for_mount(
    mount_path: &Path,
) -> Result<edenfs_client::instance::EdenFsInstance, SnapshotError> {
    let mount_path = Some(mount_path.to_path_buf());
    let config_dir = edenfs_client::utils::get_config_dir(&None, &mount_path).map_err(|err| {
        edenfs_snapshot_error(format!(
            "failed to resolve EdenFS config directory for {}: {err}",
            mount_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unknown>".to_string())
        ))
    })?;
    Ok(edenfs_client::instance::EdenFsInstance::new(
        edenfs_client::use_case::UseCaseId::Jj,
        config_dir,
        edenfs_client::utils::get_etc_eden_dir(&None),
        edenfs_client::utils::get_home_dir(&None),
    ))
}

#[cfg(fbcode_build)]
async fn read_initial_edenfs_journal_position(
    mount_point: PathBuf,
) -> Result<EdenFsJournalPosition, SnapshotError> {
    async fn read_position(mount_point: PathBuf) -> Result<EdenFsJournalPosition, SnapshotError> {
        let instance = edenfs_instance_for_mount(&mount_point)?;
        let client = instance.get_client();
        let position = client
            .get_journal_position(&Some(mount_point.clone()))
            .await
            .map_err(|err| {
                edenfs_snapshot_error(format!(
                    "failed to read initial EdenFS journal position for {}: {err}",
                    mount_point.display()
                ))
            })?;
        Ok(position.into())
    }

    if tokio::runtime::Handle::try_current().is_ok() {
        read_position(mount_point).await
    } else {
        tokio::runtime::Runtime::new()
            .map_err(|err| edenfs_snapshot_error(format!("failed to create Tokio runtime for EdenFS journal position: {err}")))?
            .block_on(read_position(mount_point))
    }
}

// ---------------------------------------------------------------------------
// WorkingCopy implementation
// ---------------------------------------------------------------------------

/// EdenFS-backed working copy.
///
/// Stores the minimal checkout state locally and delegates file-system presence
/// to EdenFS.
pub struct EdenFsWorkingCopy {
    store: Arc<Store>,
    working_copy_path: PathBuf,
    state_path: PathBuf,
    checkout_state: EdenFsCheckoutState,
    tree_cell: OnceCell<MergedTree>,
}

#[async_trait(?Send)]
impl WorkingCopy for EdenFsWorkingCopy {
    fn name(&self) -> &str {
        Self::name()
    }

    fn workspace_name(&self) -> &WorkspaceName {
        &self.checkout_state.workspace_name
    }

    fn operation_id(&self) -> &OperationId {
        &self.checkout_state.operation_id
    }

    fn tree(&self) -> Result<&MergedTree, WorkingCopyStateError> {
        self.tree_cell.get_or_try_init(|| {
            Ok(MergedTree::resolved(
                self.store.clone(),
                self.checkout_state.tree_id.clone(),
            ))
        })
    }

    fn sparse_patterns(&self) -> Result<&[RepoPathBuf], WorkingCopyStateError> {
        Ok(&self.checkout_state.sparse_patterns)
    }

    async fn start_mutation(&self) -> Result<Box<dyn LockedWorkingCopy>, WorkingCopyStateError> {
        let lock_path = self.state_path.join(LOCK_FILENAME);
        let lock = FileLock::lock(lock_path).map_err(|err| WorkingCopyStateError {
            message: "Failed to lock EdenFS working copy".to_owned(),
            err: err.into(),
        })?;

        let state = EdenFsCheckoutState::load(&self.state_path).map_err(|err| {
            WorkingCopyStateError {
                message: "Failed to read EdenFS working copy state".to_owned(),
                err: err.into(),
            }
        })?;

        let wc = Self {
            store: self.store.clone(),
            working_copy_path: self.working_copy_path.clone(),
            state_path: self.state_path.clone(),
            checkout_state: state.clone(),
            tree_cell: OnceCell::new(),
        };
        let old_operation_id = state.operation_id.clone();
        let old_tree = MergedTree::resolved(self.store.clone(), state.tree_id.clone());

        Ok(Box::new(LockedEdenFsWorkingCopy {
            wc,
            old_operation_id,
            old_tree,
            checkout_state_dirty: false,
            new_workspace_name: None,
            _lock: lock,
        }))
    }
}

impl EdenFsWorkingCopy {
    pub fn name() -> &'static str {
        "edenfs"
    }

    /// Create a new EdenFS-mode working copy with the empty tree checked out.
    pub fn init(
        store: Arc<Store>,
        working_copy_path: PathBuf,
        state_path: PathBuf,
        operation_id: OperationId,
        workspace_name: WorkspaceNameBuf,
    ) -> Result<Self, WorkingCopyStateError> {
        let checkout_state = EdenFsCheckoutState {
            operation_id,
            workspace_name,
            tree_id: store.empty_tree_id().clone(),
            sparse_patterns: vec![RepoPathBuf::root()],
            journal_position: None,
        };
        checkout_state.save(&state_path).map_err(|err| WorkingCopyStateError {
            message: "Failed to write EdenFS working copy state".to_owned(),
            err: err.into(),
        })?;
        Ok(Self {
            store,
            working_copy_path,
            state_path,
            checkout_state,
            tree_cell: OnceCell::new(),
        })
    }

    /// Load an existing EdenFS-mode working copy.
    pub fn load(
        store: Arc<Store>,
        working_copy_path: PathBuf,
        state_path: PathBuf,
    ) -> Result<Self, WorkingCopyStateError> {
        let checkout_state = EdenFsCheckoutState::load(&state_path).map_err(|err| {
            WorkingCopyStateError {
                message: "Failed to read EdenFS working copy state".to_owned(),
                err: err.into(),
            }
        })?;
        Ok(Self {
            store,
            working_copy_path,
            state_path,
            checkout_state,
            tree_cell: OnceCell::new(),
        })
    }

    #[cfg(fbcode_build)]
    pub async fn initialize_journal_position(&mut self) -> Result<(), SnapshotError> {
        let existing_paths = collect_existing_edenfs_paths(&self.working_copy_path)?;
        if !existing_paths.is_empty() {
            let tree = apply_edenfs_changes_to_tree(
                self.store.clone(),
                self.working_copy_path.clone(),
                self.store.empty_merged_tree(),
                &existing_paths,
            )
            .await?;
            let tree_id = tree.tree_ids().as_resolved().cloned().ok_or_else(|| {
                edenfs_snapshot_error(
                    "EdenFS working-copy initialization produced an unresolved tree; conflicted JJ EdenFS initialization is not supported yet",
                )
            })?;
            self.checkout_state.tree_id = tree_id;
        }
        let position = read_initial_edenfs_journal_position(self.working_copy_path.clone()).await?;
        self.checkout_state.journal_position = Some(position);
        self.checkout_state.save(&self.state_path).map_err(|err| {
            SnapshotError::WorkingCopyStateError(WorkingCopyStateError {
                message: "Failed to persist EdenFS journal position".to_owned(),
                err: err.into(),
            })
        })?;
        Ok(())
    }

    #[cfg(not(fbcode_build))]
    pub async fn initialize_journal_position(&mut self) -> Result<(), SnapshotError> {
        Err(unimplemented_snapshot_error("initialization"))
    }
}

// ---------------------------------------------------------------------------
// LockedWorkingCopy implementation
// ---------------------------------------------------------------------------

pub struct LockedEdenFsWorkingCopy {
    wc: EdenFsWorkingCopy,
    old_operation_id: OperationId,
    old_tree: MergedTree,
    checkout_state_dirty: bool,
    new_workspace_name: Option<WorkspaceNameBuf>,
    _lock: FileLock,
}

fn unimplemented_snapshot_error(operation: &'static str) -> SnapshotError {
    SnapshotError::Other {
        message: format!(
            "EdenFS working-copy {operation} requires the JJ/EdenFS materialization bridge; \
             use the local working copy until checkout, snapshot, and sparse updates are \
             wired to EdenFS journal/materialization APIs"
        ),
        err: Box::new(io::Error::new(
            io::ErrorKind::Unsupported,
            "JJ EdenFS working-copy bridge is not implemented",
        )),
    }
}

fn unimplemented_checkout_error(operation: &'static str) -> CheckoutError {
    CheckoutError::Other {
        message: format!(
            "EdenFS working-copy {operation} requires the JJ/EdenFS materialization bridge; \
             use the local working copy until checkout, snapshot, and sparse updates are \
             wired to EdenFS journal/materialization APIs"
        ),
        err: Box::new(io::Error::new(
            io::ErrorKind::Unsupported,
            "JJ EdenFS working-copy bridge is not implemented",
        )),
    }
}

#[async_trait]
impl LockedWorkingCopy for LockedEdenFsWorkingCopy {
    fn old_operation_id(&self) -> &OperationId {
        &self.old_operation_id
    }

    fn old_tree(&self) -> &MergedTree {
        &self.old_tree
    }

    #[instrument(skip_all)]
    async fn snapshot(
        &mut self,
        _options: &SnapshotOptions,
    ) -> Result<(MergedTree, SnapshotStats), SnapshotError> {
        #[cfg(fbcode_build)]
        {
            let Some(from_position) = self.wc.checkout_state.journal_position.clone() else {
                return Err(edenfs_snapshot_error(
                    "EdenFS working-copy snapshot requires an initialized EdenFS journal position; \
                     create or repair the EdenFS-backed JJ workspace before snapshotting",
                ));
            };
            let collected =
                collect_edenfs_changes_for_mount(self.wc.working_copy_path.clone(), from_position)
                    .await?;
            let EdenFsCollectedChanges {
                to_position,
                mut changed_paths,
            } = collected;
            changed_paths.retain(|change| {
                sparse_contains(change.path.as_ref(), &self.wc.checkout_state.sparse_patterns)
            });
            let store = self.wc.store.clone();
            let working_copy_path = self.wc.working_copy_path.clone();
            let base_tree = self.wc.tree()?.clone();
            let tree = apply_edenfs_changes_to_tree(
                store,
                working_copy_path,
                base_tree,
                &changed_paths,
            )
                .await?;
            let tree_id = tree.tree_ids().as_resolved().cloned().ok_or_else(|| {
                edenfs_snapshot_error(
                    "EdenFS working-copy snapshot produced an unresolved tree; conflicted JJ EdenFS snapshots are not supported yet",
                )
            })?;
            self.wc.checkout_state.tree_id = tree_id;
            self.wc.checkout_state.journal_position = Some(to_position);
            self.checkout_state_dirty = true;
            Ok((tree, SnapshotStats::default()))
        }

        #[cfg(not(fbcode_build))]
        {
            Err(unimplemented_snapshot_error("snapshot"))
        }
    }

    #[instrument(skip_all)]
    async fn check_out(&mut self, commit: &Commit) -> Result<CheckoutStats, CheckoutError> {
        let new_tree = commit.tree();
        if self.wc.tree()?.tree_ids_and_labels() != new_tree.tree_ids_and_labels() {
            #[cfg(fbcode_build)]
            {
                let old_tree = self.wc.tree()?.clone();
                let store = self.wc.store.clone();
                let working_copy_path = self.wc.working_copy_path.clone();
                let sparse_patterns = self.wc.checkout_state.sparse_patterns.clone();
                let stats = materialize_tree_to_edenfs_mount(
                    store,
                    working_copy_path.clone(),
                    old_tree,
                    new_tree.clone(),
                    &sparse_patterns,
                )
                .await?;
                let tree_id = new_tree.tree_ids().as_resolved().cloned().ok_or_else(|| {
                    CheckoutError::Other {
                        message: "EdenFS JJ checkout requires a resolved target tree".to_string(),
                        err: io::Error::new(
                            io::ErrorKind::Unsupported,
                            "unresolved target tree",
                        )
                        .into(),
                    }
                })?;
                let journal_position =
                    read_initial_edenfs_journal_position(working_copy_path)
                        .await
                        .map_err(|err| CheckoutError::Other {
                            message:
                                "Failed to read EdenFS journal position after JJ checkout"
                                    .to_string(),
                            err: err.into(),
                        })?;
                self.wc.checkout_state.tree_id = tree_id;
                self.wc.checkout_state.journal_position = Some(journal_position);
                self.checkout_state_dirty = true;
                Ok(stats)
            }

            #[cfg(not(fbcode_build))]
            {
                Err(unimplemented_checkout_error("checkout"))
            }
        } else {
            Ok(CheckoutStats::default())
        }
    }

    fn rename_workspace(&mut self, new_name: WorkspaceNameBuf) {
        self.new_workspace_name = Some(new_name);
    }

    #[instrument(skip_all)]
    async fn reset(&mut self, commit: &Commit) -> Result<(), ResetError> {
        let new_tree = commit.tree();
        self.wc.checkout_state.tree_id = new_tree
            .tree_ids()
            .as_resolved()
            .cloned()
            .unwrap_or_else(|| self.wc.store.empty_tree_id().clone());
        self.checkout_state_dirty = true;
        Ok(())
    }

    #[instrument(skip_all)]
    async fn recover(&mut self, commit: &Commit) -> Result<(), ResetError> {
        self.reset(commit).await
    }

    fn sparse_patterns(&self) -> Result<&[RepoPathBuf], WorkingCopyStateError> {
        self.wc.sparse_patterns()
    }

    #[instrument(skip_all)]
    async fn set_sparse_patterns(
        &mut self,
        new_sparse_patterns: Vec<RepoPathBuf>,
    ) -> Result<CheckoutStats, CheckoutError> {
        #[cfg(fbcode_build)]
        {
            let tree = self.wc.tree()?.clone();
            let empty_tree = self.wc.store.empty_merged_tree();
            let store = self.wc.store.clone();
            let working_copy_path = self.wc.working_copy_path.clone();
            let stats = materialize_tree_to_edenfs_mount(
                store,
                working_copy_path.clone(),
                empty_tree,
                tree,
                &new_sparse_patterns,
            )
            .await?;
            let journal_position = read_initial_edenfs_journal_position(working_copy_path)
                .await
                .map_err(|err| CheckoutError::Other {
                    message: "Failed to read EdenFS journal position after JJ sparse update"
                        .to_string(),
                    err: err.into(),
                })?;
            self.wc.checkout_state.sparse_patterns = new_sparse_patterns;
            self.wc.checkout_state.journal_position = Some(journal_position);
            self.checkout_state_dirty = true;
            Ok(stats)
        }

        #[cfg(not(fbcode_build))]
        {
            let _ = new_sparse_patterns;
            Err(unimplemented_checkout_error("sparse update"))
        }
    }

    #[instrument(skip_all)]
    async fn finish(
        mut self: Box<Self>,
        operation_id: OperationId,
    ) -> Result<Box<dyn WorkingCopy>, WorkingCopyStateError> {
        if self.checkout_state_dirty
            || self.old_operation_id != operation_id
            || self.new_workspace_name.is_some()
        {
            self.wc.checkout_state.operation_id = operation_id;
            if let Some(name) = self.new_workspace_name {
                self.wc.checkout_state.workspace_name = name;
            }
            self.wc
                .checkout_state
                .save(&self.wc.state_path)
                .map_err(|err| WorkingCopyStateError {
                    message: "Failed to write EdenFS working copy state".to_owned(),
                    err: err.into(),
                })?;
        }
        Ok(Box::new(self.wc))
    }
}

// ---------------------------------------------------------------------------
// WorkingCopyFactory implementation
// ---------------------------------------------------------------------------

/// Factory for creating and loading EdenFS-backed working copies.
pub struct EdenFsWorkingCopyFactory {}

impl WorkingCopyFactory for EdenFsWorkingCopyFactory {
    fn init_working_copy(
        &self,
        store: Arc<Store>,
        working_copy_path: PathBuf,
        state_path: PathBuf,
        operation_id: OperationId,
        workspace_name: WorkspaceNameBuf,
        _settings: &UserSettings,
    ) -> Result<Box<dyn WorkingCopy>, WorkingCopyStateError> {
        Ok(Box::new(EdenFsWorkingCopy::init(
            store,
            working_copy_path,
            state_path,
            operation_id,
            workspace_name,
        )?))
    }

    fn load_working_copy(
        &self,
        store: Arc<Store>,
        working_copy_path: PathBuf,
        state_path: PathBuf,
        _settings: &UserSettings,
    ) -> Result<Box<dyn WorkingCopy>, WorkingCopyStateError> {
        Ok(Box::new(EdenFsWorkingCopy::load(
            store,
            working_copy_path,
            state_path,
        )?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edenfs_checkout_state_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let state = EdenFsCheckoutState {
            operation_id: OperationId::new(b"op_id_bytes".to_vec()),
            workspace_name: "test-ws".into(),
            tree_id: crate::backend::TreeId::new(b"tree_bytes".to_vec()),
            sparse_patterns: vec![
                RepoPathBuf::root(),
                RepoPath::from_internal_string("foo/bar").unwrap().to_owned(),
            ],
            journal_position: Some(EdenFsJournalPosition {
                mount_generation: 42,
                sequence_number: 9001,
                snapshot_hash: b"snapshot_hash".to_vec(),
            }),
        };
        state.save(temp.path()).unwrap();
        let loaded = EdenFsCheckoutState::load(temp.path()).unwrap();
        assert_eq!(state, loaded);
    }

    #[test]
    fn test_edenfs_checkout_state_v1_loads_without_journal_position() {
        let temp = tempfile::tempdir().unwrap();
        let state_path = temp.path().join(STATE_FILENAME);
        let mut buf = Vec::new();
        buf.write_all(STATE_MAGIC).unwrap();
        buf.write_all(&1u32.to_le_bytes()).unwrap();

        let op_id = b"op_id_bytes";
        buf.write_all(&(op_id.len() as u64).to_le_bytes()).unwrap();
        buf.write_all(op_id).unwrap();

        let workspace = b"test-ws";
        buf.write_all(&(workspace.len() as u64).to_le_bytes()).unwrap();
        buf.write_all(workspace).unwrap();

        let tree_id = b"tree_bytes";
        buf.write_all(&(tree_id.len() as u64).to_le_bytes()).unwrap();
        buf.write_all(tree_id).unwrap();

        let sparse_patterns = ["", "foo/bar"];
        buf.write_all(&(sparse_patterns.len() as u64).to_le_bytes())
            .unwrap();
        for pattern in sparse_patterns {
            buf.write_all(&(pattern.len() as u64).to_le_bytes())
                .unwrap();
            buf.write_all(pattern.as_bytes()).unwrap();
        }

        std::fs::write(state_path, buf).unwrap();

        let loaded = EdenFsCheckoutState::load(temp.path()).unwrap();
        assert_eq!(loaded.operation_id, OperationId::new(op_id.to_vec()));
        assert_eq!(loaded.workspace_name.as_str(), "test-ws");
        assert_eq!(loaded.tree_id, crate::backend::TreeId::new(tree_id.to_vec()));
        assert_eq!(
            loaded.sparse_patterns,
            vec![
                RepoPathBuf::root(),
                RepoPath::from_internal_string("foo/bar").unwrap().to_owned()
            ]
        );
        assert_eq!(loaded.journal_position, None);
    }

    #[test]
    fn test_edenfs_checkout_state_bad_magic_fails() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("eden_fs_checkout");
        std::fs::write(&path, b"BAD!").unwrap();
        let result = EdenFsCheckoutState::load(temp.path());
        assert!(result.is_err());
    }
}
