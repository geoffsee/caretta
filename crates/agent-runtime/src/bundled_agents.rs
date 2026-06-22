#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledAgent {
    pub id: &'static str,
    pub binary: &'static str,
    pub package: Option<&'static str>,
    pub entrypoint: Option<&'static str>,
    pub external: bool,
}

pub const SUPPORTED_AGENTS: &[BundledAgent] = &[
    BundledAgent {
        id: "claude",
        binary: "claude",
        package: Some("@anthropic-ai/claude-code"),
        entrypoint: Some("node_modules/@anthropic-ai/claude-code/bin/claude.exe"),
        external: false,
    },
    BundledAgent {
        id: "cline",
        binary: "cline",
        package: Some("cline"),
        entrypoint: Some("node_modules/cline/dist/cli.mjs"),
        external: false,
    },
    BundledAgent {
        id: "codex",
        binary: "codex",
        package: Some("@openai/codex"),
        entrypoint: Some("node_modules/@openai/codex/bin/codex.js"),
        external: false,
    },
    BundledAgent {
        id: "copilot",
        binary: "copilot",
        package: Some("@github/copilot"),
        entrypoint: Some("node_modules/@github/copilot/npm-loader.js"),
        external: false,
    },
    BundledAgent {
        id: "cursor",
        binary: "cursor",
        package: None,
        entrypoint: None,
        external: true,
    },
    BundledAgent {
        id: "gemini",
        binary: "agy",
        package: None,
        entrypoint: Some("bin/agy"),
        external: false,
    },
    BundledAgent {
        id: "junie",
        binary: "junie",
        package: Some("@jetbrains/junie"),
        entrypoint: Some("node_modules/@jetbrains/junie/bin/index.js"),
        external: false,
    },
    BundledAgent {
        id: "xai",
        binary: "grok",
        package: None,
        entrypoint: Some("bin/grok"),
        external: false,
    },
];

/// Bundled agent CLI ids with an in-tree entrypoint, in registry order.
/// Used for scanning `node_modules` for model strings (excludes external agents such as Cursor).
pub(crate) fn iter_bundled_cli_ids() -> impl Iterator<Item = &'static str> {
    SUPPORTED_AGENTS
        .iter()
        .filter(|agent| !agent.external)
        .map(|agent| agent.id)
}
