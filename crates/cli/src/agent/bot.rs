// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use crate::agent::cmd::log;
use crate::agent::config_store::{load_bot_private_key_pem, load_bot_token};
use crate::agent::types::{BotCredentials, BotSettings};
use std::env;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

static BOT_TOKEN_CACHE: Mutex<Option<(String, Instant)>> = Mutex::new(None);
const TOKEN_CACHE_SECS: u64 = 50 * 60;

fn bot_token_cache() -> &'static Mutex<Option<(String, Instant)>> {
    &BOT_TOKEN_CACHE
}

/// Load bot credentials from environment variables.
///
/// Resolution order:
/// 1. `DEV_BOT_TOKEN` — direct token (PAT or pre-minted installation token)
/// 2. `DEV_BOT_TOKEN_PATH` — path to a file containing the token
/// 3. `DEV_BOT_APP_ID` + `DEV_BOT_INSTALLATION_ID` + `DEV_BOT_PRIVATE_KEY` — GitHub App
pub fn load_bot_credentials_from_env() -> Option<BotCredentials> {
    // Direct token from env
    if let Ok(token) = env::var("DEV_BOT_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(BotCredentials::Token(token));
        }
    }

    // Token from file
    if let Ok(path) = env::var("DEV_BOT_TOKEN_PATH")
        && let Ok(token) = std::fs::read_to_string(&path)
    {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(BotCredentials::Token(token));
        }
    }

    // GitHub App credentials
    let app_id = env::var("DEV_BOT_APP_ID").ok().filter(|s| !s.is_empty())?;
    let installation_id = env::var("DEV_BOT_INSTALLATION_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let private_key_path = env::var("DEV_BOT_PRIVATE_KEY").unwrap_or_else(|_| {
        env::var("HOME")
            .map(|h| format!("{h}/.config/caretta/dev-ui-bot.pem"))
            .unwrap_or_else(|_| ".config/caretta/dev-ui-bot.pem".to_string())
    });
    let private_key_pem = std::fs::read_to_string(&private_key_path)
        .map_err(|e| {
            log(&format!(
                "Failed to read bot private key at {private_key_path}: {e}"
            ))
        })
        .ok()?;

    Some(BotCredentials::GitHubApp {
        app_id,
        installation_id,
        private_key_pem,
    })
}

pub fn load_bot_settings(root: &str, dev_cfg: &crate::agent::types::DevConfig) -> BotSettings {
    if let Some(creds) = load_bot_credentials_from_env() {
        return BotSettings::from_credentials(&creds);
    }

    let mut settings = dev_cfg.bot.clone().into_bot_settings();
    if let Some(token) = load_bot_token(root) {
        settings.token = token;
    }
    if let Some(private_key_pem) = load_bot_private_key_pem(root) {
        settings.private_key_pem = private_key_pem;
    }
    settings
}

/// Resolve bot credentials to a usable `GH_TOKEN` value.
pub fn resolve_bot_token(creds: &BotCredentials) -> Option<String> {
    match creds {
        BotCredentials::Token(t) => Some(t.clone()),
        BotCredentials::GitHubApp {
            app_id,
            installation_id,
            private_key_pem,
        } => {
            // Check cache
            if let Ok(cache) = bot_token_cache().lock()
                && let Some((ref token, ref created_at)) = *cache
                && created_at.elapsed() < std::time::Duration::from_secs(TOKEN_CACHE_SECS)
            {
                return Some(token.clone());
            }

            let token = mint_installation_token(app_id, installation_id, private_key_pem)?;

            if let Ok(mut cache) = bot_token_cache().lock() {
                *cache = Some((token.clone(), Instant::now()));
            }

            Some(token)
        }
    }
}

/// Pass the Bearer secret to curl via stdin (`--config -`) instead of `-H argv`
/// values so secrets are never visible via `ps` / `/proc/<pid>/cmdline`.
fn curl_github_rest_bearer_stdout(
    bearer_secret: &str,
    method: &str,
    url: &str,
) -> Result<Vec<u8>, String> {
    let mut child = Command::new("curl")
        .args([
            "--config",
            "-",
            "-s",
            "-S",
            "-X",
            method,
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run curl ({e}); install curl to use GitHub App auth"))?;

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().ok_or("curl stdin unavailable")?;
        let auth_config = format!("header = \"Authorization: Bearer {bearer_secret}\"\n");
        stdin
            .write_all(auth_config.as_bytes())
            .map_err(|e| format!("could not pass auth to curl: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("curl failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "GitHub REST request failed (curl exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ));
    }

    Ok(output.stdout)
}

/// Verify a PAT / installation / OAuth token against the GitHub REST API.
pub(crate) fn verify_github_bot_token_rest(token: &str) -> Result<(), String> {
    let body =
        curl_github_rest_bearer_stdout(token.trim(), "GET", "https://api.github.com/rate_limit")?;
    let v: serde_json::Value = serde_json::from_slice(&body).map_err(|_| {
        "GitHub rate_limit response was not valid JSON (token may be invalid)".to_string()
    })?;
    if v.get("resources").is_none() {
        return Err(
            "GitHub rate_limit response missing expected fields — token may be invalid".to_string(),
        );
    }
    Ok(())
}

/// Mint a GitHub App installation access token and return failure details for diagnostics.
pub(crate) fn mint_installation_access_token(
    app_id: &str,
    installation_id: &str,
    private_key_pem: &str,
) -> Result<String, String> {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes()).map_err(|e| {
        format!(
            "Could not authenticate as GitHub App — invalid private key PEM ({e}). Check the key file or pasted PEM."
        )
    })?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("system clock error: {e}"))?
        .as_secs();

    let claims = serde_json::json!({
        "iss": app_id,
        "iat": now.saturating_sub(60),
        "exp": now + 600,
    });

    let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key).map_err(|e| {
        format!(
            "Could not authenticate as GitHub App — failed to sign JWT ({e}). Check App ID and private key pair."
        )
    })?;

    let url = format!("https://api.github.com/app/installations/{installation_id}/access_tokens");
    let body = curl_github_rest_bearer_stdout(&jwt, "POST", &url)?;

    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| format!("GitHub App token response was not valid JSON: {e}"))?;

    if let Some(token) = value.get("token").and_then(|t| t.as_str()) {
        return Ok(token.to_string());
    }

    let msg = value
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown error");
    Err(format!(
        "Could not authenticate as GitHub App — GitHub API: {msg}. Check App ID, Installation ID, and that the key belongs to this app."
    ))
}

/// Mint a GitHub App installation token via JWT + REST API.
fn mint_installation_token(
    app_id: &str,
    installation_id: &str,
    private_key_pem: &str,
) -> Option<String> {
    match mint_installation_access_token(app_id, installation_id, private_key_pem) {
        Ok(token) => Some(token),
        Err(e) => {
            log(&e);
            None
        }
    }
}
