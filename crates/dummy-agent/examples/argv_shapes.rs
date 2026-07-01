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

fn main() {
    let w = DummyAgentWrapper;
    println!("binary           : {}", w.binary());
    println!("help_args        : {:?}", w.help_args());
    println!("version_args     : {:?}", w.version_args());
    println!("model_args       : {:?}", w.model_args("dummy-model"));
    println!("project_args     : {:?}", w.project_args("/tmp"));
    println!("output_format    : {:?}", w.output_format_args("json"));
    println!("yolo_args        : {:?}", w.yolo_args());
    println!(
        "native_run_argv  : {:?}",
        w.caretta_native_run_argv("hello")
    );
}
