// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use agent_common::AgentCliAdapter;
use dummy_agent::DummyAgentWrapper;
use std::process::Command;

fn main() {
    let w = DummyAgentWrapper;
    for args in [w.help_args(), w.version_args()] {
        let status = Command::new(w.binary())
            .args(&args)
            .status()
            .expect("failed to spawn caretta-dummy-agent — is it on PATH?");
        assert!(
            status.success(),
            "{} {:?} exited with {:?}",
            w.binary(),
            args,
            status.code()
        );
        println!("ok: {} {:?}", w.binary(), args);
    }

    let probe = w.caretta_native_run_argv("probe");
    let status = Command::new(w.binary())
        .args(&probe)
        .status()
        .expect("failed to spawn caretta-dummy-agent");
    assert!(
        status.success(),
        "{} {:?} exited with {:?}",
        w.binary(),
        probe,
        status.code()
    );
    println!("ok: {} {:?}", w.binary(), probe);
}
