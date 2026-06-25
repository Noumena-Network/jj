/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

use std::env;
use std::io::Write as _;
use std::path::PathBuf;

use jj_lib::config::ConfigGetResultExt as _;
use serde::Deserialize;
use serde::Serialize;
use tracing::instrument;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error;
use crate::command_error::user_error_with_message;
use crate::ui::Ui;

const TOKEN_ENV_VARS: &[&str] = &[
    "JJAPI_TOKEN",
    "NCODE_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
    "GH_TOKEN",
    "GITHUB_TOKEN",
];

/// Manage native JJAPI repositories.
#[derive(clap::Subcommand, Clone, Debug)]
pub(crate) enum RepoCommand {
    Create(RepoCreateArgs),
    Delete(RepoDeleteArgs),
}

/// Create a native JJAPI-backed repository.
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct RepoCreateArgs {
    /// Repository slug, for example testuser2/my-repo.
    #[arg(value_name = "OWNER/REPO")]
    repo: String,

    /// JJAPI server URL. Defaults to $JJAPI_URL, $NCODE_JJAPI_URL, or jjapi.url config.
    #[arg(long, value_name = "URL")]
    server: Option<String>,

    /// Create as a public repository.
    #[arg(long, conflicts_with = "private")]
    public: bool,

    /// Create as a private repository. This is the default.
    #[arg(long)]
    private: bool,

    /// Clone the created repository immediately.
    #[arg(long)]
    clone: bool,

    /// Use an EdenFS-backed working copy when cloning.
    #[arg(long, requires = "clone")]
    eden: bool,

    /// Destination directory for --clone.
    #[arg(long, value_hint = clap::ValueHint::DirPath, requires = "clone")]
    destination: Option<String>,

    /// Sapling binary used only to create the EdenFS mount for --clone --eden.
    #[arg(long, value_name = "PATH", requires = "eden")]
    eden_sl_bin: Option<PathBuf>,

    /// Sapling/EdenAPI source used only to create the EdenFS mount for --clone --eden.
    #[arg(long, value_name = "URL", requires = "eden")]
    eden_slapi_source: Option<String>,

    /// EdenFS backing repository path used only for --clone --eden.
    #[arg(long, value_name = "PATH", requires = "eden")]
    eden_backing_repo: Option<PathBuf>,

    /// EdenFS control binary used by Sapling when creating the EdenFS mount.
    #[arg(long, value_name = "PATH", requires = "eden")]
    edenfsctl_bin: Option<PathBuf>,
}

/// Delete a native JJAPI-backed repository.
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct RepoDeleteArgs {
    /// Repository slug, for example testuser2/my-repo.
    #[arg(value_name = "OWNER/REPO")]
    repo: String,

    /// JJAPI server URL. Defaults to $JJAPI_URL, $NCODE_JJAPI_URL, or jjapi.url config.
    #[arg(long, value_name = "URL")]
    server: Option<String>,

    /// Confirm deletion.
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Serialize)]
struct RepoLifecycleRequest<'a> {
    owner: &'a str,
    name: &'a str,
    visibility: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct RepoLifecycleResponse {
    owner: String,
    name: String,
    mononoke_repo_name: String,
    repository_id: i64,
    scm_type: String,
    lifecycle: String,
    source: String,
}

#[derive(Debug, Deserialize)]
struct RepoLifecycleError {
    message: String,
}

pub(crate) async fn cmd_repo(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &RepoCommand,
) -> Result<(), CommandError> {
    match args {
        RepoCommand::Create(args) => cmd_repo_create(ui, command, args).await,
        RepoCommand::Delete(args) => cmd_repo_delete(ui, command, args).await,
    }
}

#[instrument(skip_all)]
async fn cmd_repo_create(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &RepoCreateArgs,
) -> Result<(), CommandError> {
    let (owner, name) = parse_repo_slug(&args.repo)?;
    let server = resolve_server(command, args.server.as_deref())?;
    let visibility = if args.public { "public" } else { "private" };
    let response = lifecycle_request(
        &server,
        "repo/create",
        &RepoLifecycleRequest {
            owner,
            name,
            visibility: Some(visibility),
        },
    )?;

    writeln!(
        ui.stdout(),
        "created {} repository {} (id={})",
        response.scm_type, response.mononoke_repo_name, response.repository_id
    )?;
    writeln!(ui.stdout(), "source = {}", response.source)?;

    if args.clone {
        let clone_args = super::clone::CloneArgs::native(
            response.source,
            args.destination.clone(),
            args.eden,
            args.eden_sl_bin.clone(),
            args.eden_slapi_source.clone(),
            args.eden_backing_repo.clone(),
            args.edenfsctl_bin.clone(),
        );
        super::clone::cmd_clone(ui, command, &clone_args).await?;
    }

    Ok(())
}

#[instrument(skip_all)]
async fn cmd_repo_delete(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &RepoDeleteArgs,
) -> Result<(), CommandError> {
    if !args.yes {
        return Err(user_error("refusing to delete repository without --yes"));
    }
    let (owner, name) = parse_repo_slug(&args.repo)?;
    let server = resolve_server(command, args.server.as_deref())?;
    let response = lifecycle_request(
        &server,
        "repo/delete",
        &RepoLifecycleRequest {
            owner,
            name,
            visibility: None,
        },
    )?;
    writeln!(
        ui.stdout(),
        "deleted {} repository {} (id={}, lifecycle={})",
        response.scm_type, response.mononoke_repo_name, response.repository_id, response.lifecycle
    )?;
    Ok(())
}

fn lifecycle_request(
    server: &str,
    path: &str,
    request: &RepoLifecycleRequest<'_>,
) -> Result<RepoLifecycleResponse, CommandError> {
    let token = token_from_env().ok_or_else(|| {
        user_error("missing JJAPI auth token")
            .hinted("Set JJAPI_TOKEN, NCODE_TOKEN, GH_ENTERPRISE_TOKEN, GITHUB_ENTERPRISE_TOKEN, GH_TOKEN, or GITHUB_TOKEN.")
    })?;
    let url = format!("{}/{}", server.trim_end_matches('/'), path);
    let hostname = whoami::hostname().unwrap_or_else(|_| "unknown".to_string());
    let client_info = serde_json::json!({
        "hostname": hostname,
        "tw_job": "manual",
        "sandcastle_nonce": "jj-repo-lifecycle",
    })
    .to_string();
    let body = serde_json::to_vec(request)
        .map_err(|e| user_error_with_message("failed to serialize repo lifecycle request", e))?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| user_error_with_message("failed to create Tokio runtime", e))?;
    let (status, body) = runtime.block_on(async {
        let response = reqwest::Client::new()
            .post(&url)
            .bearer_auth(token)
            .header("X-Client-Info", client_info)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| user_error_with_message(format!("failed to call {url}"), e))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| user_error_with_message(format!("failed to read {url} response"), e))?;
        Ok::<_, CommandError>((status, body))
    })?;
    if !status.is_success() {
        let message = serde_json::from_str::<RepoLifecycleError>(&body)
            .map(|error| error.message)
            .unwrap_or(body);
        return Err(user_error(format!("{url} returned {status}: {message}")));
    }
    serde_json::from_str(&body)
        .map_err(|e| user_error_with_message(format!("failed to parse {url} response"), e))
}

fn resolve_server(command: &CommandHelper, server: Option<&str>) -> Result<String, CommandError> {
    if let Some(server) = server.filter(|server| !server.trim().is_empty()) {
        return normalize_server_url(server);
    }
    if let Ok(server) = env::var("JJAPI_URL").or_else(|_| env::var("NCODE_JJAPI_URL")) {
        if !server.trim().is_empty() {
            return normalize_server_url(&server);
        }
    }
    let settings = command.settings();
    if let Some(server) = settings.get::<String>("jjapi.url").optional().ok().flatten() {
        return normalize_server_url(&server);
    }
    Err(user_error("missing JJAPI server URL")
        .hinted("Pass --server https://jj.code.staging.noumena.com or configure jjapi.url."))
}

fn normalize_server_url(server: &str) -> Result<String, CommandError> {
    let parsed = url::Url::parse(server)
        .map_err(|e| user_error_with_message(format!("invalid JJAPI server URL `{server}`"), e))?;
    let scheme = match parsed.scheme() {
        "jjapi" | "jjapi+https" | "https" => "https",
        "jjapi+http" | "http" => "http",
        scheme => {
            return Err(user_error(format!(
                "unsupported JJAPI server URL scheme `{scheme}`"
            ))
            .hinted("Expected https, http, jjapi, jjapi+https, or jjapi+http."));
        }
    };
    let host = parsed
        .host_str()
        .ok_or_else(|| user_error(format!("invalid JJAPI server URL `{server}`: missing host")))?;
    let mut out = format!("{scheme}://{host}");
    if let Some(port) = parsed.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    Ok(out)
}

fn parse_repo_slug(repo: &str) -> Result<(&str, &str), CommandError> {
    let Some((owner, name)) = repo.split_once('/') else {
        return Err(user_error(format!("invalid repository `{repo}`"))
            .hinted("Expected OWNER/REPO."));
    };
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return Err(user_error(format!("invalid repository `{repo}`"))
            .hinted("Expected OWNER/REPO."));
    }
    Ok((owner, name))
}

fn token_from_env() -> Option<String> {
    TOKEN_ENV_VARS.iter().find_map(|name| {
        env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_owner_repo_slug() {
        assert_eq!(parse_repo_slug("testuser2/repo").unwrap(), ("testuser2", "repo"));
        assert!(parse_repo_slug("repo").is_err());
        assert!(parse_repo_slug("a/b/c").is_err());
    }

    #[test]
    fn normalizes_jjapi_server_url() {
        assert_eq!(
            normalize_server_url("jjapi://jj.code.staging.noumena.com/testuser2/repo").unwrap(),
            "https://jj.code.staging.noumena.com"
        );
        assert_eq!(
            normalize_server_url("jjapi+http://localhost:8003/testuser2/repo").unwrap(),
            "http://localhost:8003"
        );
    }
}
