// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use anyhow::{Context, Result};
#[cfg(not(target_arch = "wasm32"))]
use keyring::Entry;
use std::hash::{Hash, Hasher};

const SERVICE_PREFIX: &str = "caretta";
const BOT_TOKEN_SLOT: &str = "bot-token";
const BOT_PEM_SLOT: &str = "bot-private-key-pem";
const LOCAL_INFERENCE_API_KEY_SLOT: &str = "local-inference-api-key";

fn root_scope(root: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(not(target_arch = "wasm32"))]
fn entry(root: &str, slot: &str) -> Result<Entry> {
    Entry::new(
        &format!("{SERVICE_PREFIX}.{slot}"),
        &format!("project:{}", root_scope(root)),
    )
    .context("failed to open OS credential store entry")
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_secret(root: &str, slot: &str) -> Option<String> {
    let entry = entry(root, slot).ok()?;
    match entry.get_password() {
        Ok(secret) if !secret.trim().is_empty() => Some(secret),
        Ok(_) => None,
        Err(_) => None,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn load_secret(_root: &str, _slot: &str) -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn store_secret(root: &str, slot: &str, value: &str) -> Result<()> {
    let entry = entry(root, slot)?;
    entry
        .set_password(value)
        .with_context(|| format!("failed to store secret for {slot}"))
}

#[cfg(target_arch = "wasm32")]
pub fn store_secret(_root: &str, _slot: &str, _value: &str) -> Result<()> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_secret(root: &str, slot: &str) -> Result<()> {
    let entry = entry(root, slot)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(_) => Ok(()),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn clear_secret(_root: &str, _slot: &str) -> Result<()> {
    Ok(())
}

pub fn load_bot_token(root: &str) -> Option<String> {
    load_secret(root, BOT_TOKEN_SLOT)
}

pub fn store_bot_token(root: &str, token: &str) -> Result<()> {
    store_secret(root, BOT_TOKEN_SLOT, token)
}

pub fn clear_bot_token(root: &str) -> Result<()> {
    clear_secret(root, BOT_TOKEN_SLOT)
}

pub fn load_bot_private_key_pem(root: &str) -> Option<String> {
    load_secret(root, BOT_PEM_SLOT)
}

pub fn store_bot_private_key_pem(root: &str, pem: &str) -> Result<()> {
    store_secret(root, BOT_PEM_SLOT, pem)
}

pub fn clear_bot_private_key_pem(root: &str) -> Result<()> {
    clear_secret(root, BOT_PEM_SLOT)
}

pub fn load_local_inference_api_key(root: &str) -> Option<String> {
    load_secret(root, LOCAL_INFERENCE_API_KEY_SLOT)
}

pub fn store_local_inference_api_key(root: &str, api_key: &str) -> Result<()> {
    store_secret(root, LOCAL_INFERENCE_API_KEY_SLOT, api_key)
}

pub fn clear_local_inference_api_key(root: &str) -> Result<()> {
    clear_secret(root, LOCAL_INFERENCE_API_KEY_SLOT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_scope_is_stable() {
        let a = root_scope("/tmp/project-a");
        let b = root_scope("/tmp/project-a");
        let c = root_scope("/tmp/project-b");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
