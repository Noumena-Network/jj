/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

use tracing::instrument;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::command_error::user_error_with_message;
use crate::ui::Ui;

/// Clone a native JJAPI-backed repository.
///
/// This is the user-facing wrapper for:
///
/// `jj init --backend mononoke --source <SOURCE> [--working-copy edenfs] <DEST>`
///
/// It intentionally does not use the Git compatibility path.
#[derive(clap::Args, Clone, Debug)]
pub(crate) struct CloneArgs {
    /// External JJAPI source URL, e.g. jjapi://jj.code.staging.noumena.com/owner/repo
    #[arg(value_name = "SOURCE")]
    source: String,

    /// Destination directory. Defaults to the repository name from SOURCE.
    #[arg(value_hint = clap::ValueHint::DirPath)]
    destination: Option<String>,

    /// Use an EdenFS-backed working copy.
    #[arg(long)]
    eden: bool,

    /// Sapling binary used only to create the EdenFS mount for --eden.
    ///
    /// Defaults to $JJ_EDEN_SL_BIN, then $NCODE_SL_BIN, then `sl` on PATH.
    #[arg(long, value_name = "PATH")]
    eden_sl_bin: Option<PathBuf>,

    /// Sapling/EdenAPI source used only to create the EdenFS mount for --eden.
    ///
    /// Defaults to $JJ_EDEN_SLAPI_SOURCE, then a derived slapi source.
    #[arg(long, value_name = "URL")]
    eden_slapi_source: Option<String>,

    /// EdenFS backing repository path used only for --eden.
    ///
    /// Defaults to $JJ_EDEN_BACKING_REPO, then ~/.eden-backing-repos/<owner%2Frepo>.
    #[arg(long, value_name = "PATH")]
    eden_backing_repo: Option<PathBuf>,

    /// EdenFS control binary used by Sapling when creating the EdenFS mount.
    ///
    /// Defaults to $JJ_EDENFSCTL_BIN, then $NCODE_EDENFSCTL_BIN, then `edenfsctl` on PATH.
    #[arg(long, value_name = "PATH", requires = "eden")]
    edenfsctl_bin: Option<PathBuf>,
}

impl CloneArgs {
    pub(crate) fn native(
        source: String,
        destination: Option<String>,
        eden: bool,
        eden_sl_bin: Option<PathBuf>,
        eden_slapi_source: Option<String>,
        eden_backing_repo: Option<PathBuf>,
        edenfsctl_bin: Option<PathBuf>,
    ) -> Self {
        Self {
            source,
            destination,
            eden,
            eden_sl_bin,
            eden_slapi_source,
            eden_backing_repo,
            edenfsctl_bin,
        }
    }
}

#[instrument(skip_all)]
pub(crate) async fn cmd_clone(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &CloneArgs,
) -> Result<(), CommandError> {
    let destination = args
        .destination
        .clone()
        .unwrap_or_else(|| destination_from_source(&args.source));
    if args.eden {
        bootstrap_edenfs_mount(args, &destination)?;
    }
    let working_copy = args.eden.then(|| "edenfs".to_string());
    let init_args =
        super::init::InitArgs::mononoke(destination, args.source.clone(), working_copy);
    super::init::cmd_init(ui, command, &init_args).await
}

fn bootstrap_edenfs_mount(args: &CloneArgs, destination: &str) -> Result<(), CommandError> {
    let destination_path = Path::new(destination);
    if destination_path.join(".eden").exists() {
        return Ok(());
    }

    let sl_bin = args
        .eden_sl_bin
        .clone()
        .or_else(|| env::var_os("JJ_EDEN_SL_BIN").map(PathBuf::from))
        .or_else(|| env::var_os("NCODE_SL_BIN").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("sl"));
    let slapi_source = args
        .eden_slapi_source
        .clone()
        .or_else(|| env::var("JJ_EDEN_SLAPI_SOURCE").ok())
        .unwrap_or_else(|| derive_slapi_source(&args.source));
    let backing_repo = args
        .eden_backing_repo
        .clone()
        .or_else(|| env::var_os("JJ_EDEN_BACKING_REPO").map(PathBuf::from))
        .unwrap_or_else(|| default_eden_backing_repo(&args.source));
    let edenfsctl_bin = args
        .edenfsctl_bin
        .clone()
        .or_else(|| env::var_os("JJ_EDENFSCTL_BIN").map(PathBuf::from))
        .or_else(|| env::var_os("NCODE_EDENFSCTL_BIN").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("edenfsctl"));

    let status = Command::new(&sl_bin)
        .arg("--config")
        .arg("clone.use-rust=True")
        .arg("--config")
        .arg("commands.force-rust=clone")
        .arg("--config")
        .arg(format!("edenfs.command={}", edenfsctl_bin.display()))
        .arg("clone")
        .arg("--eden")
        .arg("--eden-backing-repo")
        .arg(&backing_repo)
        .arg(&slapi_source)
        .arg(destination)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| user_error_with_message("Failed to run Sapling EdenFS mount bootstrap", e))?;
    if !status.success() {
        return Err(user_error_with_message(
            "Failed to create EdenFS mount for JJ clone",
            std::io::Error::other(format!(
                "{} exited with status {status}",
                sl_bin.display()
            )),
        ));
    }
    if !destination_path.join(".eden").exists() {
        return Err(user_error_with_message(
            "Failed to create EdenFS mount for JJ clone",
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{} is not an EdenFS checkout after clone", destination),
            ),
        ));
    }
    Ok(())
}

fn destination_from_source(source: &str) -> String {
    source
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            segment
                .strip_suffix(".git")
                .unwrap_or(segment)
                .to_string()
        })
        .unwrap_or_else(|| "repo".to_string())
}

fn derive_slapi_source(source: &str) -> String {
    let Ok(url) = url::Url::parse(source) else {
        return source.to_string();
    };
    let scheme = match url.scheme() {
        "jjapi" | "jjapi+http" => "slapi+http",
        "jjapi+https" => "slapi+https",
        _ => return source.to_string(),
    };
    let Some(host) = url.host_str() else {
        return source.to_string();
    };
    let sl_host = host
        .strip_prefix("jj.")
        .map(|suffix| format!("sl.{suffix}"))
        .unwrap_or_else(|| host.to_string());
    let mut out = format!("{scheme}://{sl_host}");
    if let Some(port) = url.port() {
        out.push(':');
        out.push_str(&port.to_string());
    }
    out.push_str(url.path());
    out
}

fn default_eden_backing_repo(source: &str) -> PathBuf {
    let repo_name = url::Url::parse(source)
        .ok()
        .map(|url| url.path().trim_matches('/').replace('/', "%2F"))
        .filter(|repo| !repo.is_empty())
        .unwrap_or_else(|| destination_from_source(source));
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".eden-backing-repos")
        .join(repo_name)
}

#[cfg(test)]
mod tests {
    use super::destination_from_source;

    #[test]
    fn destination_uses_last_source_segment() {
        assert_eq!(
            destination_from_source("jjapi://jj.code.staging.noumena.com/testuser2/repo"),
            "repo"
        );
    }

    #[test]
    fn destination_strips_git_suffix() {
        assert_eq!(
            destination_from_source("jjapi://jj.code.staging.noumena.com/testuser2/repo.git"),
            "repo"
        );
    }

    #[test]
    fn derives_slapi_source_from_jjapi_source() {
        assert_eq!(
            derive_slapi_source("jjapi+http://jj.code.staging.noumena.com/testuser2/repo"),
            "slapi+http://sl.code.staging.noumena.com/testuser2/repo"
        );
        assert_eq!(
            derive_slapi_source("jjapi://jj.code.staging.noumena.com/testuser2/repo"),
            "slapi+http://sl.code.staging.noumena.com/testuser2/repo"
        );
        assert_eq!(
            derive_slapi_source("jjapi+https://jj.code.staging.noumena.com/testuser2/repo"),
            "slapi+https://sl.code.staging.noumena.com/testuser2/repo"
        );
    }
}
