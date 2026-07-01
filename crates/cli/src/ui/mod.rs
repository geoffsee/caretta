// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

pub mod components;
pub mod discovery;
pub mod editor;
pub mod personas;
pub mod security;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
pub mod sidebar;
pub mod statusbar;

pub use discovery::DiscoveryPanel;
pub use discovery::DiscoveryWorkspace;
pub use editor::Editor;
pub use sidebar::Sidebar;
pub use statusbar::Statusbar;
