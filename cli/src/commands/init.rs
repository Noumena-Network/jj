/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::io::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use jj_lib::config::ConfigGetResultExt as _;
use jj_lib::file_util;
use jj_lib::ref_name::WorkspaceNameBuf;
use jj_lib::repo::ReadonlyRepo;
use jj_lib::signing::Signer;
use jj_lib::workspace::Workspace;
use tracing::instrument;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::cli_error;
use crate::command_error::user_error_with_message;
use crate::ui::Ui;

/// Create a new repo in the given directory
///
/// If the given directory does not exist, it will be created. If no directory is
/// given, the current directory is used.
///
/// By default, a local `simple` backend is used. Use `--backend mononoke` to
/// create a workspace backed by the Mononoke remote backend (requires
/// `jjapi.*` config or `JJAPI_TOKEN`/`NCODE_TOKEN` environment variables).
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct InitArgs {
    /// The destination directory
    #[arg(default_value = ".", value_hint = clap::ValueHint::DirPath)]
    destination: String,

    /// The backend to use for the new repo
    #[arg(long, value_name = "BACKEND")]
    backend: Option<String>,

    /// External JJAPI source URL, e.g. jjapi://jj.code.staging.noumena.com/owner/repo
    #[arg(long, value_name = "URL")]
    source: Option<String>,

    /// Working-copy implementation for Mononoke-backed repositories: local or edenfs
    #[arg(long, value_name = "TYPE")]
    working_copy: Option<String>,
}

impl InitArgs {
    pub(crate) fn mononoke(
        destination: String,
        source: String,
        working_copy: Option<String>,
    ) -> Self {
        Self {
            destination,
            backend: Some("mononoke".to_string()),
            source: Some(source),
            working_copy,
        }
    }
}

#[instrument(skip_all)]
pub(crate) async fn cmd_init(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &InitArgs,
) -> Result<(), CommandError> {
    if command.global_args().no_integrate_operation {
        return Err(cli_error("--no-integrate-operation is not respected"));
    }
    if command.global_args().ignore_working_copy {
        return Err(cli_error("--ignore-working-copy is not respected"));
    }
    if command.global_args().at_operation.is_some() {
        return Err(cli_error("--at-op is not respected"));
    }

    let cwd = command.cwd();
    let wc_path = cwd.join(&args.destination);
    let wc_path = file_util::create_or_reuse_dir(&wc_path)
        .and_then(|_| dunce::canonicalize(wc_path))
        .map_err(|e| user_error_with_message("Failed to create workspace", e))?;

    let backend = args.backend.as_deref().unwrap_or("simple");

    if backend == "mononoke" {
        return init_mononoke(
            ui,
            command,
            &wc_path,
            args.source.as_deref(),
            args.working_copy.as_deref(),
        )
        .await;
    }

    if args.working_copy.is_some() {
        return Err(user_error_with_message(
            "--working-copy is only supported with --backend mononoke",
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "unsupported working-copy"),
        ));
    }

    if backend != "simple" {
        return Err(user_error_with_message(
            format!("Unknown backend: {backend}"),
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "expected 'simple' or 'mononoke'",
            ),
        ));
    }

    // Simple backend (default).
    let settings = command.settings_for_new_workspace(ui, &wc_path)?.0;
    Workspace::init_simple(&settings, &wc_path)
        .await
        .map_err(|e| user_error_with_message("Failed to initialise workspace", e))?;

    let relative_wc_path = file_util::relative_path(cwd, &wc_path);
    writeln!(
        ui.status(),
        "Initialized repo in \"{}\"",
        relative_wc_path.display()
    )?;
    Ok(())
}

async fn init_mononoke(
    ui: &mut Ui,
    command: &CommandHelper,
    wc_path: &std::path::Path,
    source: Option<&str>,
    working_copy: Option<&str>,
) -> Result<(), CommandError> {
    use jj_lib::backend::{CommitId, TreeId, ChangeId};
    use jj_lib::eden_fs_working_copy::EdenFsWorkingCopy;
    use jj_lib::eden_fs_working_copy::EdenFsWorkingCopyFactory;
    use jj_lib::object_id::ObjectId as _;
    use jj_lib::op_store::{View, ViewId, Operation, OperationId, RootOperationData};
    use jj_edenapi::JjRemoteApi as _;
    use jj_edenapi_types::wire::tree::WireWriteTreeRequest;

    let settings = command.settings();
    let source_config = source
        .map(parse_jjapi_source)
        .transpose()?;
    let http_url: String = match source_config.as_ref() {
        Some(config) => config.url.clone(),
        None => settings
            .get("jjapi.url")
            .map_err(|e| user_error_with_message("jjapi.url not configured", e))?,
    };
    let repo_name: String = match source_config.as_ref() {
        Some(config) => config.repo_name.clone(),
        None => settings
            .get("jjapi.reponame")
            .map_err(|e| user_error_with_message("jjapi.reponame not configured", e))?,
    };

    // Optional root IDs. Commit IDs are 32-byte Blake2 IDs; tree IDs are
    // 64-byte Blake2b-512 IDs in the JJ wire/backend contract.
    let root_commit_id: CommitId = settings
        .get("jjapi.root_commit_id")
        .optional()
        .ok()
        .flatten()
        .and_then(|hex: String| CommitId::try_from_hex(&hex))
        .unwrap_or_else(|| CommitId::from_bytes(&[0; 32]));
    let configured_empty_tree_id: Option<TreeId> = settings
        .get("jjapi.empty_tree_id")
        .optional()
        .ok()
        .flatten()
        .and_then(|hex: String| TreeId::try_from_hex(&hex));
    let root_change_id = ChangeId::from_bytes(&[0; 16]);
    let workspace_name: WorkspaceNameBuf = settings
        .get("jjapi.workspace")
        .optional()
        .ok()
        .flatten()
        .unwrap_or_else(|| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or_default();
            format!("workspace-{}-{}", std::process::id(), now)
        })
        .into();

    let has_token = ["JJAPI_TOKEN", "NCODE_TOKEN", "GH_ENTERPRISE_TOKEN", "GH_TOKEN"]
        .iter()
        .any(|name| std::env::var(name).map(|v| !v.trim().is_empty()).unwrap_or(false));
    let token_env_hint = if has_token {
        ""
    } else {
        " (set JJAPI_TOKEN, NCODE_TOKEN, GH_ENTERPRISE_TOKEN, or GH_TOKEN)"
    };

    let client = jj_edenapi::Builder::new()
        .server_url(http_url.parse().map_err(|e| {
            user_error_with_message(format!("invalid jjapi.url{}", token_env_hint), e)
        })?)
        .repo_name(&repo_name)
        .build()
        .map_err(|e| user_error_with_message(format!("failed to build jjapi client{}", token_env_hint), e))?;

    let empty_tree_id: TreeId = match configured_empty_tree_id {
        Some(id) => id,
        None => {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| user_error_with_message("Failed to create Tokio runtime", e))?;
            let tree_id = rt.block_on(async {
                let response = client
                    .write_tree(WireWriteTreeRequest {
                    repo: repo_name.clone(),
                    tree: crate::jjapi_backend::tree_to_wire(&jj_lib::backend::Tree::default()),
                })
                .await
                    .map_err(|e| user_error_with_message("Failed to write remote empty tree", e))?;
                let entries = response
                    .flatten()
                    .await
                    .map_err(|e| user_error_with_message("Failed to read remote empty tree response", e))?;
                let entry = entries
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        user_error_with_message("Failed to write remote empty tree", "empty response")
                    })?;
                Ok::<_, CommandError>(entry.tree_id)
            })?;
            TreeId::from_bytes(&tree_id)
        }
    };

    let client: std::sync::Arc<dyn jj_edenapi::JjRemoteApi> = std::sync::Arc::new(client);

    // Compute deterministic root view / operation IDs.
    let root_view = View::make_root(root_commit_id.clone());
    let root_view_id = ViewId::from_bytes(
        jj_lib::content_hash::blake2b_hash(&root_view).to_vec().as_slice(),
    );
    let root_op = Operation::make_root(root_view_id.clone());
    let root_operation_id = OperationId::from_bytes(
        jj_lib::content_hash::blake2b_hash(&root_op).to_vec().as_slice(),
    );
    let root_data = RootOperationData { root_commit_id: root_commit_id.clone() };

    let backend_initializer: &jj_lib::repo::BackendInitializer = &|_settings, _store_path| {
        Ok(Box::new(crate::jjapi_backend::JjapiBackend::new(
            repo_name.clone(),
            root_commit_id.clone(),
            root_change_id.clone(),
            empty_tree_id.clone(),
            client.clone(),
        )))
    };

    let op_store_initializer: &jj_lib::repo::OpStoreInitializer =
        &|_settings, _store_path, root_data| {
            Ok(Box::new(crate::jjapi_op_store::JjapiOpStore::new(
                repo_name.clone(),
                workspace_name.clone(),
                root_data,
                root_operation_id.clone(),
                root_view_id.clone(),
                client.clone(),
            )))
        };

    let op_heads_store_initializer: &jj_lib::repo::OpHeadsStoreInitializer =
        &|_settings, _store_path| {
            Ok(Box::new(crate::jjapi_op_store::JjapiOpStore::new(
                repo_name.clone(),
                workspace_name.clone(),
                root_data.clone(),
                root_operation_id.clone(),
                root_view_id.clone(),
                client.clone(),
            )))
        };

    let signer = Signer::from_settings(settings)
        .map_err(|e| user_error_with_message("Failed to create signer", e))?;

    let use_edenfs_working_copy = match working_copy {
        None | Some("local") => false,
        Some("edenfs") => true,
        Some(value) => {
            return Err(user_error_with_message(
                format!("unsupported Mononoke working copy `{value}`"),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "expected 'local' or 'edenfs'",
                ),
            ));
        }
    };
    let edenfs_factory = EdenFsWorkingCopyFactory {};
    let local_factory = jj_lib::workspace::default_working_copy_factory();
    let working_copy_factory: &dyn jj_lib::working_copy::WorkingCopyFactory =
        if use_edenfs_working_copy {
            &edenfs_factory
        } else {
            &*local_factory
        };

    let (mut workspace, repo) = Workspace::init_with_factories(
        settings,
        wc_path,
        backend_initializer,
        signer,
        op_store_initializer,
        op_heads_store_initializer,
        ReadonlyRepo::default_index_store_initializer(),
        ReadonlyRepo::default_submodule_store_initializer(),
        working_copy_factory,
        workspace_name.clone(),
    )
    .await
    .map_err(|e| user_error_with_message("Failed to initialise Mononoke workspace", e))?;
    let imported_bookmarks = import_remote_bookmarks_into_initial_view(
        client.clone(),
        &repo_name,
        repo.clone(),
    )
    .await?;
    if imported_bookmarks > 0 {
        writeln!(
            ui.status(),
            "Imported {imported_bookmarks} remote Mononoke bookmarks"
        )?;
    }
    let config_text = format!(
        r#"[jjapi]
url = "{}"
reponame = "{}"
workspace = "{}"
root_commit_id = "{}"
empty_tree_id = "{}"
"#,
        http_url,
        repo_name,
        workspace_name.as_str(),
        root_commit_id.hex(),
        empty_tree_id.hex(),
    );
    // Persist into both workspace and repo config scopes.  The workspace config
    // is useful for humans; the store factories are invoked with repo settings.
    let jj_config_path = wc_path.join(".jj").join("config.toml");
    std::fs::write(&jj_config_path, &config_text)
        .map_err(|e| user_error_with_message("Failed to write .jj/config.toml", e))?;
    let jj_repo_config_path = wc_path.join(".jj").join("repo").join("config.toml");
    std::fs::write(&jj_repo_config_path, config_text)
        .map_err(|e| user_error_with_message("Failed to write .jj/repo/config.toml", e))?;

    if use_edenfs_working_copy {
        let edenfs_working_copy = workspace
            .working_copy_mut()
            .downcast_mut::<EdenFsWorkingCopy>()
            .ok_or_else(|| {
                user_error_with_message(
                    "Failed to initialise EdenFS working copy",
                    std::io::Error::other("working copy factory did not return EdenFsWorkingCopy"),
                )
            })?;
        edenfs_working_copy
            .initialize_journal_position()
            .await
            .map_err(|e| {
                user_error_with_message("Failed to initialise EdenFS journal position", e)
            })?;
    }

    writeln!(
        ui.status(),
        "Initialized Mononoke-backed JJ repo{} in \"{}\"",
        if use_edenfs_working_copy {
            " with EdenFS working copy"
        } else {
            ""
        },
        wc_path.display(),
    )?;

    Ok(())
}

async fn import_remote_bookmarks_into_initial_view(
    client: std::sync::Arc<dyn jj_edenapi::JjRemoteApi>,
    repo_name: &str,
    repo: std::sync::Arc<ReadonlyRepo>,
) -> Result<usize, CommandError> {
    use jj_lib::backend::CommitId;
    use jj_lib::object_id::ObjectId as _;
    use jj_lib::op_store::RefTarget;
    use jj_lib::ref_name::RefNameBuf;
    use jj_lib::repo::Repo as _;
    use jj_edenapi_types::wire::bookmark::WireListBookmarksRequest;

    let response = client
        .list_bookmarks(WireListBookmarksRequest {
            repo: repo_name.to_string(),
            prefix: None,
        })
        .await
        .map_err(|e| user_error_with_message("Failed to list remote JJ bookmarks", e))?;
    let entries = response
        .flatten()
        .await
        .map_err(|e| user_error_with_message("Failed to read remote JJ bookmark response", e))?;

    let mut tx = repo.start_transaction();
    let mut imported = 0usize;
    for entry in entries {
        for bookmark in entry.bookmarks {
            let Some(commit_id) = bookmark.commit_id else {
                continue;
            };
            let local_name = bookmark
                .name
                .strip_prefix("heads/")
                .unwrap_or(&bookmark.name)
                .to_string();
            if local_name.is_empty() {
                continue;
            }
            let name = RefNameBuf::from(local_name);
            let target = CommitId::from_bytes(&commit_id);
            let commit = repo
                .store()
                .get_commit(&target)
                .map_err(|e| {
                    user_error_with_message(
                        format!("Failed to read remote JJ bookmark target {}", target.hex()),
                        e,
                    )
                })?;
            tx.repo_mut()
                .add_head(&commit)
                .await
                .map_err(|e| {
                    user_error_with_message(
                        format!("Failed to index remote JJ bookmark target {}", target.hex()),
                        e,
                    )
                })?;
            tx.repo_mut()
                .set_local_bookmark_target(&name, RefTarget::normal(target));
            imported += 1;
        }
    }
    if imported > 0 {
        tx.commit(format!("import {imported} remote JJ bookmarks"))
            .await
            .map_err(|e| user_error_with_message("Failed to persist remote JJ bookmarks", e))?;
    }
    Ok(imported)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct JjapiSourceConfig {
    url: String,
    repo_name: String,
}

fn parse_jjapi_source(source: &str) -> Result<JjapiSourceConfig, CommandError> {
    let source_url = url::Url::parse(source)
        .map_err(|e| user_error_with_message(format!("invalid jjapi source URL `{source}`"), e))?;
    let scheme = match source_url.scheme() {
        "jjapi" | "jjapi+https" => "https",
        "jjapi+http" => "http",
        scheme => {
            return Err(user_error_with_message(
                format!("unsupported jjapi source URL scheme `{scheme}`"),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "expected jjapi, jjapi+https, or jjapi+http",
                ),
            ));
        }
    };
    let host = source_url.host_str().ok_or_else(|| {
        user_error_with_message(
            format!("invalid jjapi source URL `{source}`"),
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing host"),
        )
    })?;
    let repo_name = source_url.path().trim_matches('/').to_string();
    if repo_name.is_empty() {
        return Err(user_error_with_message(
            format!("invalid jjapi source URL `{source}`"),
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing repository path"),
        ));
    }
    let mut url = url::Url::parse(&format!("{scheme}://{host}/"))
        .map_err(|e| user_error_with_message(format!("invalid jjapi source URL `{source}`"), e))?;
    if let Some(port) = source_url.port() {
        url.set_port(Some(port)).map_err(|_| {
            user_error_with_message(
                format!("invalid jjapi source URL `{source}`"),
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid port"),
            )
        })?;
    }

    Ok(JjapiSourceConfig {
        url: url.to_string().trim_end_matches('/').to_string(),
        repo_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jjapi_https_source() {
        assert_eq!(
            parse_jjapi_source("jjapi://jj.code.staging.noumena.com/noumena/ncode").unwrap(),
            JjapiSourceConfig {
                url: "https://jj.code.staging.noumena.com".to_string(),
                repo_name: "noumena/ncode".to_string(),
            }
        );
    }

    #[test]
    fn parses_jjapi_http_source_with_port() {
        assert_eq!(
            parse_jjapi_source("jjapi+http://localhost:8003/testuser2/repo").unwrap(),
            JjapiSourceConfig {
                url: "http://localhost:8003".to_string(),
                repo_name: "testuser2/repo".to_string(),
            }
        );
    }

    #[test]
    fn rejects_jjapi_source_without_repo_path() {
        assert!(parse_jjapi_source("jjapi://jj.code.staging.noumena.com/").is_err());
    }
}
