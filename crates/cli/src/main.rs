// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

//! Standalone caretta binary. The real entry point lives in `lib.rs` so
//! library consumers (e.g. project-specific shims that want to inject custom
//! `Config` fields) can call [`caretta::run_with_overrides`] directly.

fn main() {
    caretta::run();
}
