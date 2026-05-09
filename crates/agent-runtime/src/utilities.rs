// `build.rs` includes this file without `cfg(test)`; only part of the API is
// needed to regenerate `available-models.json`, so many items are unused there.
#![allow(dead_code)]

use regex::{Captures, Regex};

/// A named regex fragment with a human-readable explanation.
#[derive(Debug, Clone)]
pub struct PatternPart {
    pub name: &'static str,
    pub description: &'static str,
    pub pattern: &'static str,
}

impl PatternPart {
    pub const fn new(name: &'static str, description: &'static str, pattern: &'static str) -> Self {
        Self {
            name,
            description,
            pattern,
        }
    }
}

/// Result of matching a model string.
#[derive(Debug, Clone)]
pub struct ModelMatch<'a> {
    pub input: &'a str,
    pub matched_text: &'a str,
    pub part: &'static PatternPart,
}

/// Self-explaining regex utility for LLM model identifiers.
#[derive(Debug, Clone)]
pub struct ModelRegex {
    regex: Regex,
    pattern: String,
}

fn model_match_from_captures<'a>(captures: Captures<'a>, input: &'a str) -> Option<ModelMatch<'a>> {
    let whole = captures.get(0)?;

    for part in ModelRegex::parts() {
        if captures.name(part.name).is_some() {
            return Some(ModelMatch {
                input,
                matched_text: whole.as_str(),
                part,
            });
        }
    }

    None
}

impl ModelRegex {
    pub fn new() -> Result<Self, regex::Error> {
        let pattern = Self::build_pattern();
        let regex = Regex::new(&pattern)?;

        Ok(Self { regex, pattern })
    }

    /// The final compiled regex pattern.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// All named model pattern parts.
    pub fn parts() -> &'static [PatternPart] {
        &MODEL_PATTERN_PARTS
    }

    /// Human-readable explanation of every supported model pattern.
    pub fn explain() -> String {
        Self::parts()
            .iter()
            .map(|part| {
                format!(
                    "## {}\n{}\n\nRegex:\n{}\n",
                    part.name, part.description, part.pattern
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check whether any supported model pattern appears in the input.
    pub fn is_match(&self, input: &str) -> bool {
        self.regex.is_match(input)
    }

    /// Return the first match and explain which pattern matched it.
    pub fn explain_match<'a>(&self, input: &'a str) -> Option<ModelMatch<'a>> {
        model_match_from_captures(self.regex.captures(input)?, input)
    }

    /// Return all matches and explain which pattern matched each one.
    pub fn explain_matches<'a>(&self, input: &'a str) -> Vec<ModelMatch<'a>> {
        self.regex
            .captures_iter(input)
            .filter_map(|caps| model_match_from_captures(caps, input))
            .collect()
    }

    fn build_pattern() -> String {
        let alternatives = Self::parts()
            .iter()
            .map(|part| format!("(?P<{}>{})", part.name, part.pattern))
            .collect::<Vec<_>>()
            .join("|");

        format!(r"\b(?:{})\b", alternatives)
    }
}

/// The actual model pattern registry.
///
/// Add new providers or model families here.
/// Each entry gets:
/// - a named capture group
/// - a description
/// - a reusable pattern fragment
///
/// Ordering matters:
/// more specific patterns should come before more generic ones.
/// For example, `codex` comes before `gpt` so `gpt-5.3-codex`
/// gets classified as Codex instead of generic GPT.
static MODEL_PATTERN_PARTS: [PatternPart; 8] = [
    PatternPart::new(
        "claude_instant",
        "Matches Claude Instant models like `claude-instant-1`, `claude-instant-1.2`, or `claude-instant-1-100k`.",
        r"claude-instant-\d+(?:\.\d+)?(?:-\d+k)?",
    ),
    PatternPart::new(
        "claude_numbered_family",
        "Matches numbered Claude family models like `claude-3-sonnet`, `claude-3-5-haiku`, `claude-3-7-sonnet`, or `claude-4-opus-20241022-v2`.",
        r"claude-[234](?:-[357])?-(?:haiku|sonnet|opus)(?:-\d{8})?(?:-v\d+)?",
    ),
    PatternPart::new(
        "claude_family_first",
        "Matches Claude models where the family name comes before the version, like `claude-haiku`, `claude-haiku-3-5`, `claude-sonnet-3-7`, or `claude-opus-4`.",
        r"claude-(?:haiku|sonnet|opus)(?:-\d(?:-\d)?)?(?:-\d{8})?(?:-v\d+)?",
    ),
    PatternPart::new(
        "gemini",
        "Matches Gemini models like `gemini-1.5-flash`, `gemini-2.0-pro`, `gemini-2.5-flash-preview-05-20`, or similar dated/preview variants.",
        r"gemini-\d+(?:\.\d+)?-(?:flash|pro)(?:-[a-z0-9]+)*(?:-\d{2}-\d{2}|-\d{3}|-\d{4})?",
    ),
    PatternPart::new(
        "codex",
        "Matches Codex models like `codex-mini-latest`, `gpt-5.3-codex`, `gpt-5.3-codex-spark`, or similar Codex-specialized variants.",
        r"(?:codex-[a-z0-9]+(?:-[a-z0-9]+)*|gpt-\d+(?:\.\d+)?(?:-[a-z0-9]+)*-codex(?:-[a-z0-9]+)*)",
    ),
    PatternPart::new(
        "gpt",
        "Matches GPT models like `gpt-4`, `gpt-4o`, `gpt-4.1-mini`, `gpt-4-turbo`, or dated variants like `gpt-4-0125-preview`.",
        r"gpt-\d+(?:\.\d+)?(?:-[a-z0-9]+)*(?:-\d{4}-\d{2}-\d{2}|[a-z])?",
    ),
    PatternPart::new(
        "grok",
        "Matches Grok models like `grok-1`, `grok-2`, `grok-2-mini`, or `grok-3-beta`.",
        r"grok-\d+(?:-\d+)?(?:-[a-z0-9]+)*",
    ),
    PatternPart::new(
        "bare_alias",
        "Matches bare shorthand aliases like `sonnet`, `opus`, `haiku`, `gpt`, `grok`, or `codex`.",
        r"(?:sonnet|opus|haiku|gpt|grok|codex)",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_claude_models() {
        let models = ModelRegex::new().unwrap();

        assert!(models.is_match("claude-instant-1"));
        assert!(models.is_match("claude-instant-1.2"));
        assert!(models.is_match("claude-instant-1-100k"));

        assert!(models.is_match("claude-3-sonnet"));
        assert!(models.is_match("claude-3-5-haiku"));
        assert!(models.is_match("claude-3-7-sonnet"));
        assert!(models.is_match("claude-4-opus"));
        assert!(models.is_match("claude-4-opus-20241022-v2"));

        assert!(models.is_match("claude-haiku"));
        assert!(models.is_match("claude-haiku-3-5"));
        assert!(models.is_match("claude-sonnet-3-7"));
        assert!(models.is_match("claude-opus-4"));
    }

    #[test]
    fn matches_gemini_models() {
        let models = ModelRegex::new().unwrap();

        assert!(models.is_match("gemini-1.5-flash"));
        assert!(models.is_match("gemini-2.0-pro"));
        assert!(models.is_match("gemini-2.5-flash-preview-05-20"));
        assert!(models.is_match("gemini-2.5-pro-preview-123"));
        assert!(models.is_match("gemini-2.5-flash-2025"));
    }

    #[test]
    fn matches_codex_models() {
        let models = ModelRegex::new().unwrap();

        assert!(models.is_match("codex-mini-latest"));
        assert!(models.is_match("codex-cli"));
        assert!(models.is_match("gpt-5.3-codex"));
        assert!(models.is_match("gpt-5.3-codex-spark"));
    }

    #[test]
    fn matches_gpt_models() {
        let models = ModelRegex::new().unwrap();

        assert!(models.is_match("gpt-4"));
        assert!(models.is_match("gpt-4o"));
        assert!(models.is_match("gpt-4.1"));
        assert!(models.is_match("gpt-4.1-mini"));
        assert!(models.is_match("gpt-4-0125-preview"));
        assert!(models.is_match("gpt-4-2024-08-06"));
    }

    #[test]
    fn matches_grok_models() {
        let models = ModelRegex::new().unwrap();

        assert!(models.is_match("grok-1"));
        assert!(models.is_match("grok-2"));
        assert!(models.is_match("grok-2-mini"));
        assert!(models.is_match("grok-3-beta"));
    }

    #[test]
    fn matches_bare_aliases() {
        let models = ModelRegex::new().unwrap();

        assert!(models.is_match("sonnet"));
        assert!(models.is_match("opus"));
        assert!(models.is_match("haiku"));
        assert!(models.is_match("gpt"));
        assert!(models.is_match("grok"));
        assert!(models.is_match("codex"));
    }

    #[test]
    fn explains_first_match() {
        let models = ModelRegex::new().unwrap();

        let result = models
            .explain_match("please use claude-3-5-sonnet")
            .unwrap();

        assert_eq!(result.matched_text, "claude-3-5-sonnet");
        assert_eq!(result.part.name, "claude_numbered_family");
    }

    #[test]
    fn explains_codex_before_generic_gpt() {
        let models = ModelRegex::new().unwrap();

        let result = models.explain_match("use gpt-5.3-codex-spark").unwrap();

        assert_eq!(result.matched_text, "gpt-5.3-codex-spark");
        assert_eq!(result.part.name, "codex");
    }

    #[test]
    fn explains_multiple_matches() {
        let models = ModelRegex::new().unwrap();

        let results = models
            .explain_matches("use gpt-4o, gemini-1.5-flash, codex-mini-latest, and grok-2-mini");

        let matched = results
            .iter()
            .map(|result| result.matched_text)
            .collect::<Vec<_>>();

        assert_eq!(
            matched,
            vec![
                "gpt-4o",
                "gemini-1.5-flash",
                "codex-mini-latest",
                "grok-2-mini"
            ]
        );
    }
}
