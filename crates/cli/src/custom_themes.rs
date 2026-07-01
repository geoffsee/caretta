// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

/// A complete terminal/editor color theme definition.
///
/// Each field stores a hex color string (e.g. `"#1a1b26"`).
/// The struct covers backgrounds, foregrounds, ANSI-style accent slots,
/// UI chrome, and syntax highlighting categories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,

    // ── Core surfaces ───────────────────────────────────────────────
    pub bg_primary: &'static str,
    pub bg_secondary: &'static str,
    pub bg_tertiary: &'static str,

    // ── Foregrounds ─────────────────────────────────────────────────
    pub fg_primary: &'static str,
    pub fg_secondary: &'static str,
    pub fg_muted: &'static str,

    // ── Accent palette (8 slots, loosely ANSI-mapped) ───────────────
    pub red: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub blue: &'static str,
    pub magenta: &'static str,
    pub cyan: &'static str,
    pub orange: &'static str,
    pub purple: &'static str,

    // ── UI chrome ───────────────────────────────────────────────────
    pub border: &'static str,
    pub selection: &'static str,
    pub cursor: &'static str,
    pub line_highlight: &'static str,

    // ── Syntax highlighting ─────────────────────────────────────────
    pub keyword: &'static str,
    pub string: &'static str,
    pub function: &'static str,
    pub r#type: &'static str,
    pub comment: &'static str,
    pub constant: &'static str,
    pub operator: &'static str,
}

impl Theme {
    // ── 1. Tokyo Night ──────────────────────────────────────────────
    pub fn tokyo_night() -> Self {
        Self {
            name: "Tokyo Night",
            bg_primary: "#1a1b26",
            bg_secondary: "#24283b",
            bg_tertiary: "#292e42",
            fg_primary: "#c0caf5",
            fg_secondary: "#a9b1d6",
            fg_muted: "#565f89",
            red: "#f7768e",
            green: "#9ece6a",
            yellow: "#e0af68",
            blue: "#7aa2f7",
            magenta: "#bb9af7",
            cyan: "#7dcfff",
            orange: "#ff9e64",
            purple: "#9d7cd8",
            border: "#3b4261",
            selection: "#33467c",
            cursor: "#c0caf5",
            line_highlight: "#292e42",
            keyword: "#9d7cd8",
            string: "#9ece6a",
            function: "#7aa2f7",
            r#type: "#2ac3de",
            comment: "#565f89",
            constant: "#ff9e64",
            operator: "#89ddff",
        }
    }

    // ── 2. Catppuccin Mocha ─────────────────────────────────────────
    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "Catppuccin Mocha",
            bg_primary: "#1e1e2e",
            bg_secondary: "#181825",
            bg_tertiary: "#313244",
            fg_primary: "#cdd6f4",
            fg_secondary: "#bac2de",
            fg_muted: "#6c7086",
            red: "#f38ba8",
            green: "#a6e3a1",
            yellow: "#f9e2af",
            blue: "#89b4fa",
            magenta: "#f5c2e7",
            cyan: "#94e2d5",
            orange: "#fab387",
            purple: "#cba6f7",
            border: "#45475a",
            selection: "#45475a",
            cursor: "#f5e0dc",
            line_highlight: "#313244",
            keyword: "#cba6f7",
            string: "#a6e3a1",
            function: "#89b4fa",
            r#type: "#f9e2af",
            comment: "#6c7086",
            constant: "#fab387",
            operator: "#89dceb",
        }
    }

    // ── 3. Dracula ──────────────────────────────────────────────────
    pub fn dracula() -> Self {
        Self {
            name: "Dracula",
            bg_primary: "#282a36",
            bg_secondary: "#21222c",
            bg_tertiary: "#343746",
            fg_primary: "#f8f8f2",
            fg_secondary: "#e2e2dc",
            fg_muted: "#6272a4",
            red: "#ff5555",
            green: "#50fa7b",
            yellow: "#f1fa8c",
            blue: "#6272a4",
            magenta: "#ff79c6",
            cyan: "#8be9fd",
            orange: "#ffb86c",
            purple: "#bd93f9",
            border: "#44475a",
            selection: "#44475a",
            cursor: "#f8f8f2",
            line_highlight: "#343746",
            keyword: "#ff79c6",
            string: "#f1fa8c",
            function: "#50fa7b",
            r#type: "#8be9fd",
            comment: "#6272a4",
            constant: "#bd93f9",
            operator: "#ff79c6",
        }
    }

    // ── 4. Nord ─────────────────────────────────────────────────────
    pub fn nord() -> Self {
        Self {
            name: "Nord",
            bg_primary: "#2e3440",
            bg_secondary: "#3b4252",
            bg_tertiary: "#434c5e",
            fg_primary: "#eceff4",
            fg_secondary: "#e5e9f0",
            fg_muted: "#4c566a",
            red: "#bf616a",
            green: "#a3be8c",
            yellow: "#ebcb8b",
            blue: "#81a1c1",
            magenta: "#b48ead",
            cyan: "#88c0d0",
            orange: "#d08770",
            purple: "#b48ead",
            border: "#4c566a",
            selection: "#434c5e",
            cursor: "#d8dee9",
            line_highlight: "#3b4252",
            keyword: "#81a1c1",
            string: "#a3be8c",
            function: "#88c0d0",
            r#type: "#8fbcbb",
            comment: "#616e88",
            constant: "#b48ead",
            operator: "#81a1c1",
        }
    }

    // ── 5. Gruvbox Dark ─────────────────────────────────────────────
    pub fn gruvbox_dark() -> Self {
        Self {
            name: "Gruvbox Dark",
            bg_primary: "#282828",
            bg_secondary: "#1d2021",
            bg_tertiary: "#3c3836",
            fg_primary: "#ebdbb2",
            fg_secondary: "#d5c4a1",
            fg_muted: "#665c54",
            red: "#fb4934",
            green: "#b8bb26",
            yellow: "#fabd2f",
            blue: "#83a598",
            magenta: "#d3869b",
            cyan: "#8ec07c",
            orange: "#fe8019",
            purple: "#d3869b",
            border: "#504945",
            selection: "#504945",
            cursor: "#ebdbb2",
            line_highlight: "#3c3836",
            keyword: "#fb4934",
            string: "#b8bb26",
            function: "#fabd2f",
            r#type: "#83a598",
            comment: "#928374",
            constant: "#d3869b",
            operator: "#fe8019",
        }
    }

    // ── 6. Solarized Dark ───────────────────────────────────────────
    pub fn solarized_dark() -> Self {
        Self {
            name: "Solarized Dark",
            bg_primary: "#002b36",
            bg_secondary: "#073642",
            bg_tertiary: "#073642",
            fg_primary: "#839496",
            fg_secondary: "#93a1a1",
            fg_muted: "#586e75",
            red: "#dc322f",
            green: "#859900",
            yellow: "#b58900",
            blue: "#268bd2",
            magenta: "#d33682",
            cyan: "#2aa198",
            orange: "#cb4b16",
            purple: "#6c71c4",
            border: "#073642",
            selection: "#073642",
            cursor: "#839496",
            line_highlight: "#073642",
            keyword: "#859900",
            string: "#2aa198",
            function: "#268bd2",
            r#type: "#b58900",
            comment: "#586e75",
            constant: "#cb4b16",
            operator: "#839496",
        }
    }

    // ── 7. One Dark Pro ─────────────────────────────────────────────
    pub fn one_dark_pro() -> Self {
        Self {
            name: "One Dark Pro",
            bg_primary: "#282c34",
            bg_secondary: "#21252b",
            bg_tertiary: "#2c313a",
            fg_primary: "#abb2bf",
            fg_secondary: "#9da5b4",
            fg_muted: "#5c6370",
            red: "#e06c75",
            green: "#98c379",
            yellow: "#e5c07b",
            blue: "#61afef",
            magenta: "#c678dd",
            cyan: "#56b6c2",
            orange: "#d19a66",
            purple: "#c678dd",
            border: "#3e4452",
            selection: "#3e4452",
            cursor: "#528bff",
            line_highlight: "#2c313a",
            keyword: "#c678dd",
            string: "#98c379",
            function: "#61afef",
            r#type: "#e5c07b",
            comment: "#5c6370",
            constant: "#d19a66",
            operator: "#56b6c2",
        }
    }

    // ── 8. Rosé Pine ────────────────────────────────────────────────
    pub fn rose_pine() -> Self {
        Self {
            name: "Rosé Pine",
            bg_primary: "#191724",
            bg_secondary: "#1f1d2e",
            bg_tertiary: "#26233a",
            fg_primary: "#e0def4",
            fg_secondary: "#908caa",
            fg_muted: "#6e6a86",
            red: "#eb6f92",
            green: "#31748f",
            yellow: "#f6c177",
            blue: "#9ccfd8",
            magenta: "#c4a7e7",
            cyan: "#9ccfd8",
            orange: "#f6c177",
            purple: "#c4a7e7",
            border: "#403d52",
            selection: "#403d52",
            cursor: "#524f67",
            line_highlight: "#26233a",
            keyword: "#31748f",
            string: "#f6c177",
            function: "#eb6f92",
            r#type: "#9ccfd8",
            comment: "#6e6a86",
            constant: "#c4a7e7",
            operator: "#908caa",
        }
    }

    // ── 9. Synthwave '84 ────────────────────────────────────────────
    pub fn synthwave_84() -> Self {
        Self {
            name: "Synthwave '84",
            bg_primary: "#262335",
            bg_secondary: "#1e1a2b",
            bg_tertiary: "#2e2a3f",
            fg_primary: "#ffffff",
            fg_secondary: "#bbbbbb",
            fg_muted: "#848bbd",
            red: "#fe4450",
            green: "#72f1b8",
            yellow: "#fede5d",
            blue: "#36f9f6",
            magenta: "#ff7edb",
            cyan: "#36f9f6",
            orange: "#f97e72",
            purple: "#c792ea",
            border: "#3b375e",
            selection: "#3b375e",
            cursor: "#ff7edb",
            line_highlight: "#2e2a3f",
            keyword: "#fede5d",
            string: "#ff8b39",
            function: "#36f9f6",
            r#type: "#ff7edb",
            comment: "#848bbd",
            constant: "#f97e72",
            operator: "#fede5d",
        }
    }

    // ── 10. GitHub Dark ─────────────────────────────────────────────
    pub fn github_dark() -> Self {
        Self {
            name: "GitHub Dark",
            bg_primary: "#0d1117",
            bg_secondary: "#161b22",
            bg_tertiary: "#21262d",
            fg_primary: "#c9d1d9",
            fg_secondary: "#b1bac4",
            fg_muted: "#484f58",
            red: "#ff7b72",
            green: "#7ee787",
            yellow: "#e3b341",
            blue: "#79c0ff",
            magenta: "#d2a8ff",
            cyan: "#a5d6ff",
            orange: "#ffa657",
            purple: "#d2a8ff",
            border: "#30363d",
            selection: "#264f78",
            cursor: "#c9d1d9",
            line_highlight: "#161b22",
            keyword: "#ff7b72",
            string: "#a5d6ff",
            function: "#d2a8ff",
            r#type: "#ffa657",
            comment: "#8b949e",
            constant: "#79c0ff",
            operator: "#ff7b72",
        }
    }

    /// All built-in themes, in presentation order.
    pub fn all() -> [Self; 10] {
        [
            Self::tokyo_night(),
            Self::catppuccin_mocha(),
            Self::dracula(),
            Self::nord(),
            Self::gruvbox_dark(),
            Self::solarized_dark(),
            Self::one_dark_pro(),
            Self::rose_pine(),
            Self::synthwave_84(),
            Self::github_dark(),
        ]
    }

    /// Look up a theme by (case-insensitive) name.
    pub fn by_name(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        Self::all()
            .into_iter()
            .find(|t| t.name.to_lowercase() == lower)
    }

    /// Emit the theme as CSS custom properties.
    pub fn to_css_vars(&self) -> String {
        format!(
            r#":root {{
  --bg-primary: {bg1};
  --bg-secondary: {bg2};
  --bg-tertiary: {bg3};
  --fg-primary: {fg1};
  --fg-secondary: {fg2};
  --fg-muted: {fgm};
  --red: {red};
  --green: {grn};
  --yellow: {yel};
  --blue: {blu};
  --magenta: {mag};
  --cyan: {cyn};
  --orange: {org};
  --purple: {pur};
  --border: {bdr};
  --selection: {sel};
  --cursor: {cur};
  --line-highlight: {lh};
  --keyword: {kw};
  --string: {str};
  --function: {fn_};
  --type: {ty};
  --comment: {cmt};
  --constant: {cst};
  --operator: {op};
}}"#,
            bg1 = self.bg_primary,
            bg2 = self.bg_secondary,
            bg3 = self.bg_tertiary,
            fg1 = self.fg_primary,
            fg2 = self.fg_secondary,
            fgm = self.fg_muted,
            red = self.red,
            grn = self.green,
            yel = self.yellow,
            blu = self.blue,
            mag = self.magenta,
            cyn = self.cyan,
            org = self.orange,
            pur = self.purple,
            bdr = self.border,
            sel = self.selection,
            cur = self.cursor,
            lh = self.line_highlight,
            kw = self.keyword,
            str = self.string,
            fn_ = self.function,
            ty = self.r#type,
            cmt = self.comment,
            cst = self.constant,
            op = self.operator,
        )
    }
}

// ── Display ─────────────────────────────────────────────────────────
impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{name}  bg:{bg}  fg:{fg}  accent:[{r} {g} {b} {y} {m} {c}]",
            name = self.name,
            bg = self.bg_primary,
            fg = self.fg_primary,
            r = self.red,
            g = self.green,
            b = self.blue,
            y = self.yellow,
            m = self.magenta,
            c = self.cyan,
        )
    }
}

// ── Quick smoke test ────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_themes_have_unique_names() {
        let themes = Theme::all();
        for (i, a) in themes.iter().enumerate() {
            for b in &themes[i + 1..] {
                assert_ne!(a.name, b.name, "duplicate theme name");
            }
        }
    }

    #[test]
    fn lookup_by_name() {
        assert_eq!(Theme::by_name("dracula").unwrap().bg_primary, "#282a36");
        assert!(Theme::by_name("nonexistent").is_none());
    }

    #[test]
    fn css_vars_contain_all_keys() {
        let css = Theme::tokyo_night().to_css_vars();
        assert!(css.contains("--bg-primary"));
        assert!(css.contains("--operator"));
    }
}
