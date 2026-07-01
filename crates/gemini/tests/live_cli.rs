// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use agent_common::AgentCliAdapter;
use gemini::GeminiWrapper;
use std::process::Command;

#[test]
fn cli_help_and_version_are_compatible() {
    if std::env::var_os("CARETTA_LIVE_CLI_TESTS").is_none() {
        return;
    }

    let wrapper = GeminiWrapper;
    for args in [wrapper.help_args(), wrapper.version_args()] {
        let status = Command::new(wrapper.binary())
            .args(args)
            .status()
            .expect("failed to spawn provider binary");
        assert!(status.success());
    }
}
