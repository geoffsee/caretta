// Copyright (c) 2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

//! Minimal stand-in provider CLI for `dummy-agent` live tests and CI verification.

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("caretta-dummy-agent: expected at least one argument (try --help)");
        return ExitCode::from(2);
    }

    if matches!(args.as_slice(), [h] if h == "--help" || h == "-h") {
        println!("caretta-dummy-agent - test double for caretta CI");
        return ExitCode::SUCCESS;
    }

    if matches!(args.as_slice(), [v] if v == "--version" || v == "-V") {
        println!("caretta-dummy-agent {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    // Accept any argv the adapter might emit so `live_probe` and future checks stay non-fatal.
    ExitCode::SUCCESS
}
