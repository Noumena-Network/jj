// Copyright (c) 2026 Noumena, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Write as _;

use jj_edenapi::JjRemoteApi as _;
use jj_edenapi_types::wire::bookmark::WireCreateBookmarkRequest;
use jj_edenapi_types::wire::bookmark::WireMoveBookmarkRequest;
use jj_edenapi_types::wire::bookmark::WireResolveBookmarkRequest;
use jj_lib::backend::CommitId;
use jj_lib::object_id::ObjectId as _;
use jj_lib::repo::Repo as _;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::command_error::user_error_with_message;
use crate::ui::Ui;

/// Publish a native JJ revision to a Mononoke bookmark.
///
/// This is the JJ-native peer of `sl push --to`, not `jj git push`.
#[derive(clap::Args, Clone, Debug)]
pub struct PushArgs {
    /// Remote Mononoke bookmark to create or update.
    #[arg(long = "to", value_name = "BOOKMARK")]
    to: String,

    /// Revision to publish.
    #[arg(long = "revision", short = 'r', default_value = "@")]
    revision: RevisionArg,

    /// Create the remote bookmark if it does not exist.
    #[arg(long)]
    create: bool,

    /// Permit a non-fast-forward bookmark move once the backend supports it.
    #[arg(long)]
    force: bool,

    /// Resolve and print the intended native operation without mutating remote state.
    #[arg(long)]
    dry_run: bool,
}

/// Land a native JJ stack onto a Mononoke bookmark.
///
/// This maps to the same Mononoke land-stack semantics used by SLAPI
/// `/land/async`: `(base, head]` is pushrebased onto the target bookmark.
#[derive(clap::Args, Clone, Debug)]
pub struct LandArgs {
    /// Remote Mononoke bookmark to land onto.
    #[arg(long = "to", value_name = "BOOKMARK")]
    to: String,

    /// Stack head revision.
    #[arg(long)]
    head: RevisionArg,

    /// Public base revision. The landed stack is `(base, head]`.
    #[arg(long)]
    base: RevisionArg,

    /// Resolve and print the intended native operation without mutating remote state.
    #[arg(long)]
    dry_run: bool,
}

pub async fn cmd_push(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &PushArgs,
) -> Result<(), CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;
    let commit = workspace_command
        .resolve_single_rev(ui, &args.revision)
        .await?;
    let commit_id = commit.id().hex();

    if args.dry_run {
        writeln!(
            ui.stdout(),
            "would publish JJ commit {commit_id} to Mononoke bookmark `{}`{}{}",
            args.to,
            if args.create { " with create" } else { "" },
            if args.force { " with force" } else { "" },
        )?;
        return Ok(());
    }

    let (repo_name, client) = load_jjapi_push_client(workspace_command.settings())?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| user_error_with_message("failed to create JJAPI Tokio runtime", e))?;
    let resolved = runtime
        .block_on(async {
            let response = client
                .resolve_bookmark(WireResolveBookmarkRequest {
                    repo: repo_name.clone(),
                    name: args.to.clone(),
                })
                .await?;
            response.flatten().await
        })
        .map_err(|e| user_error_with_message("failed to resolve remote JJ bookmark", e))?;
    let old_target = resolved
        .into_iter()
        .next()
        .and_then(|response| response.commit_id);
    let target_commit_id = commit.id().as_bytes().to_vec();

    match old_target {
        None => {
            if !args.create {
                return Err(user_error(format!(
                    "remote bookmark `{}` does not exist",
                    args.to
                ))
                .hinted("Pass --create to create the bookmark."));
            }
            runtime
                .block_on(async {
                    let response = client
                        .create_bookmark(WireCreateBookmarkRequest {
                            repo: repo_name,
                            name: args.to.clone(),
                            target_commit_id,
                        })
                        .await?;
                    response.flatten().await
                })
                .map_err(|e| user_error_with_message("failed to create remote JJ bookmark", e))?;
            writeln!(
                ui.stdout(),
                "created Mononoke bookmark `{}` at JJ commit {commit_id}",
                args.to
            )?;
        }
        Some(old_target) => {
            let old_commit_id = CommitId::from_bytes(&old_target);
            if &old_commit_id == commit.id() {
                writeln!(
                    ui.stdout(),
                    "Mononoke bookmark `{}` already points at JJ commit {commit_id}",
                    args.to
                )?;
                return Ok(());
            }
            if !args.force {
                let is_fast_forward = workspace_command
                    .repo()
                    .index()
                    .is_ancestor(&old_commit_id, commit.id())
                    .map_err(|e| {
                        user_error_with_message(
                            "failed to verify native JJ push fast-forward safety",
                            e,
                        )
                    })?;
                if !is_fast_forward {
                    return Err(user_error(format!(
                        "refusing non-fast-forward update of Mononoke bookmark `{}`",
                        args.to
                    ))
                    .hinted(format!(
                        "Remote bookmark points at {}; requested target is {}.",
                        old_commit_id.hex(),
                        commit_id
                    ))
                    .hinted("Pass --force only after verifying the remote bookmark history."));
                }
            }
            runtime
                .block_on(async {
                    let response = client
                        .move_bookmark(WireMoveBookmarkRequest {
                            repo: repo_name,
                            name: args.to.clone(),
                            old_target_commit_id: old_target,
                            new_target_commit_id: target_commit_id,
                        })
                        .await?;
                    response.flatten().await
                })
                .map_err(|e| user_error_with_message("failed to move remote JJ bookmark", e))?;
            writeln!(
                ui.stdout(),
                "moved Mononoke bookmark `{}` to JJ commit {commit_id}",
                args.to
            )?;
        }
    }

    Ok(())
}

pub async fn cmd_land(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &LandArgs,
) -> Result<(), CommandError> {
    let workspace_command = command.workspace_helper(ui).await?;
    let head = workspace_command.resolve_single_rev(ui, &args.head).await?;
    let base = workspace_command.resolve_single_rev(ui, &args.base).await?;
    let head_id = head.id().hex();
    let base_id = base.id().hex();

    if args.dry_run {
        writeln!(
            ui.stdout(),
            "would land JJ stack ({base_id}, {head_id}] onto Mononoke bookmark `{}`",
            args.to
        )?;
        return Ok(());
    }

    Err(missing_land_api_error(&args.to, &base_id, &head_id))
}

fn load_jjapi_push_client(
    settings: &jj_lib::settings::UserSettings,
) -> Result<(String, jj_edenapi::Client), CommandError> {
    let repo_name = settings
        .get::<String>("jjapi.reponame")
        .map_err(|e| user_error_with_message("jjapi.reponame is not configured", e))?;
    let http_url = settings
        .get::<String>("jjapi.url")
        .map_err(|e| user_error_with_message("jjapi.url is not configured", e))?;
    let client = jj_edenapi::Builder::new()
        .server_url(
            http_url
                .parse()
                .map_err(|e| user_error_with_message("invalid jjapi.url", e))?,
        )
        .repo_name(&repo_name)
        .build()
        .map_err(|e| user_error_with_message("failed to build JJAPI client", e))?;
    Ok((repo_name, client))
}

fn missing_land_api_error(bookmark: &str, base_id: &str, head_id: &str) -> CommandError {
    user_error(format!(
        "native JJ land is blocked: JJAPI has no land-stack client/server endpoint for JJ stack ({base_id}, {head_id}] onto `{bookmark}`"
    ))
    .hinted(
        "Required backend API delta: expose a JJAPI land_stack endpoint, or a checked JJ commit-id -> Mononoke/Hg identity mapping plus SLAPI /land/async client path.",
    )
    .hinted(
        "Required semantics: match SLAPI LandStackRequest over Mononoke pushrebase: bookmark, head, base, pushvars, async token polling, returned old-to-new commit mapping, and local operation/bookmark reconciliation.",
    )
    .hinted("No remote state was changed.")
}
