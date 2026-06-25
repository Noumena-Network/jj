/*
 * Copyright (c) Noumena, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 */

//! Auth header injection for the JJ EdenAPI client.
//!
//! JJAPI uses the same GHES/Authy credential authority as the GH and web
//! clients. Environment token precedence mirrors GitHub CLI for operator and
//! automation compatibility, with JJ/Noumena-specific variables first so
//! callers can override auth for JJ without perturbing a shell's GH session.

use std::collections::HashMap;

const AUTHORIZATION: &str = "Authorization";
const BEARER: &str = "Bearer";

const TOKEN_ENV_VARS: &[&str] = &[
    "JJAPI_TOKEN",
    "NCODE_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
    "GH_TOKEN",
    "GITHUB_TOKEN",
];

pub fn authentication_headers() -> HashMap<String, String> {
    authentication_headers_from_env(std::env::vars())
}

fn authentication_headers_from_env<I, K, V>(vars: I) -> HashMap<String, String>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let vars: HashMap<String, String> = vars
        .into_iter()
        .map(|(key, value)| (key.as_ref().to_string(), value.as_ref().to_string()))
        .collect();

    for name in TOKEN_ENV_VARS {
        let Some(token) = vars.get(*name).map(|value| value.trim()) else {
            continue;
        };
        if token.is_empty() {
            continue;
        }

        return [(AUTHORIZATION.to_string(), format!("{BEARER} {token}"))]
            .into_iter()
            .collect();
    }

    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(vars: &[(&str, &str)]) -> HashMap<String, String> {
        authentication_headers_from_env(vars.iter().copied())
    }

    #[test]
    fn returns_no_headers_when_no_token_env_is_present() {
        assert!(headers(&[]).is_empty());
    }

    #[test]
    fn injects_bearer_header_from_jjapi_token() {
        let headers = headers(&[("JJAPI_TOKEN", "jj-token")]);
        assert_eq!(
            headers.get(AUTHORIZATION).map(String::as_str),
            Some("Bearer jj-token")
        );
    }

    #[test]
    fn follows_jj_then_enterprise_then_gh_token_precedence() {
        let jj_headers = headers(&[
            ("GITHUB_TOKEN", "github"),
            ("GH_TOKEN", "gh"),
            ("GITHUB_ENTERPRISE_TOKEN", "github-enterprise"),
            ("GH_ENTERPRISE_TOKEN", "gh-enterprise"),
            ("NCODE_TOKEN", "ncode"),
            ("JJAPI_TOKEN", "jj"),
        ]);
        assert_eq!(
            jj_headers.get(AUTHORIZATION).map(String::as_str),
            Some("Bearer jj")
        );

        let enterprise_headers = headers(&[
            ("GITHUB_TOKEN", "github"),
            ("GH_TOKEN", "gh"),
            ("GITHUB_ENTERPRISE_TOKEN", "github-enterprise"),
            ("GH_ENTERPRISE_TOKEN", "gh-enterprise"),
        ]);
        assert_eq!(
            enterprise_headers.get(AUTHORIZATION).map(String::as_str),
            Some("Bearer gh-enterprise")
        );
    }

    #[test]
    fn ignores_empty_or_whitespace_tokens() {
        let headers = headers(&[
            ("JJAPI_TOKEN", " "),
            ("GH_ENTERPRISE_TOKEN", ""),
            ("GH_TOKEN", " gh-token "),
        ]);
        assert_eq!(
            headers.get(AUTHORIZATION).map(String::as_str),
            Some("Bearer gh-token")
        );
    }
}
