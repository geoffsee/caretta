// Copyright (c) 2024-2026 Geoff Seemueller
//
// Licensed under the MIT License or Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// See LICENSE-MIT or LICENSE-APACHE for the full license text.
//
// Additionally, this file is subject to the Revenue Sharing Agreement terms
// as defined in REVENUE-SHARING.md for covered organizations.

use dioxus::prelude::*;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Critical => write!(f, "CRITICAL"),
            Severity::High => write!(f, "HIGH"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::Low => write!(f, "LOW"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

impl Severity {
    pub fn css_class(&self) -> &'static str {
        match self {
            Severity::Critical => "sev-critical",
            Severity::High => "sev-high",
            Severity::Medium => "sev-medium",
            Severity::Low => "sev-low",
            Severity::Info => "sev-info",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum FindingStatus {
    Pass,
    Fail,
    Warning,
}

impl fmt::Display for FindingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FindingStatus::Pass => write!(f, "PASS"),
            FindingStatus::Fail => write!(f, "FAIL"),
            FindingStatus::Warning => write!(f, "WARN"),
        }
    }
}

impl FindingStatus {
    pub fn css_class(&self) -> &'static str {
        match self {
            FindingStatus::Pass => "status-pass",
            FindingStatus::Fail => "status-fail",
            FindingStatus::Warning => "status-warn",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SecurityFinding {
    pub category: &'static str,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub status: FindingStatus,
    pub remediation: Option<String>,
}

// ── Scanner ─────────────────────────────────────────────────────────────────
//
// The scanner runs against files the user has declared as security-relevant in
// `caretta.toml` under `[security_scan].paths`. The interface is intentionally
// declarative: the user names what matters, and the scanner runs a small set of
// broadly-applicable checks against each declared path. Repo-wide hygiene
// checks (.gitignore coverage, SECURITY.md presence) run regardless.

fn read_source(cache: &mut HashMap<String, String>, root: &str, relative: &str) -> Option<String> {
    if let Some(cached) = cache.get(relative) {
        return Some(cached.clone());
    }
    let path = Path::new(root).join(relative);
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            cache.insert(relative.to_string(), content.clone());
            Some(content)
        }
        Err(_) => None,
    }
}

pub fn run_security_scan(
    root: &str,
    targets: &crate::agent::types::ScanTargets,
) -> Vec<SecurityFinding> {
    let mut findings = Vec::new();
    let mut cache = HashMap::new();

    scan_gitignore_hygiene(root, &mut findings);
    scan_security_disclosure_policy(root, &mut findings);

    if targets.paths.is_empty() {
        findings.push(SecurityFinding {
            category: "Configuration",
            severity: Severity::Medium,
            title: "No security-scan paths declared".into(),
            description:
                "`[security_scan].paths` in caretta.toml is empty, so the per-file checks have nothing to read. Without declared targets, agents end up guessing what matters."
                    .into(),
            status: FindingStatus::Warning,
            remediation: Some(
                "Declare the files this project considers security-relevant under `[security_scan].paths` in caretta.toml.".into(),
            ),
        });
    } else {
        for relative in &targets.paths {
            match read_source(&mut cache, root, relative) {
                Some(source) => {
                    scan_secrets_in(relative, &source, &mut findings);
                    scan_weak_crypto_in(relative, &source, &mut findings);
                    scan_plaintext_http_in(relative, &source, &mut findings);
                }
                None => {
                    findings.push(SecurityFinding {
                        category: "Configuration",
                        severity: Severity::Low,
                        title: format!("Declared scan path not found: {relative}"),
                        description: format!(
                            "`{relative}` is declared in [security_scan].paths but does not exist on disk."
                        ),
                        status: FindingStatus::Warning,
                        remediation: Some(
                            "Remove the path from caretta.toml or fix it to point at a real file.".into(),
                        ),
                    });
                }
            }
        }
    }

    // Sort: failures first, then by severity.
    findings.sort_by_key(|f| {
        let status_ord = match f.status {
            FindingStatus::Fail => 0,
            FindingStatus::Warning => 1,
            FindingStatus::Pass => 2,
        };
        let sev_ord = match f.severity {
            Severity::Critical => 0,
            Severity::High => 1,
            Severity::Medium => 2,
            Severity::Low => 3,
            Severity::Info => 4,
        };
        (status_ord, sev_ord)
    });

    findings
}

// ── Repo-wide hygiene checks ────────────────────────────────────────────────

fn scan_gitignore_hygiene(root: &str, findings: &mut Vec<SecurityFinding>) {
    let path = Path::new(root).join(".gitignore");
    let Ok(content) = std::fs::read_to_string(&path) else {
        findings.push(SecurityFinding {
            category: "Repository Hygiene",
            severity: Severity::Medium,
            title: ".gitignore is missing".into(),
            description:
                "No .gitignore at the repository root. Common secret-bearing files (.env, private keys) can be committed accidentally."
                    .into(),
            status: FindingStatus::Warning,
            remediation: Some(
                "Add a .gitignore covering at least .env, *.pem, *.key, and id_rsa-style files.".into(),
            ),
        });
        return;
    };

    let want = [
        (".env", "environment files often holding API keys"),
        ("*.pem", "PEM-encoded keys and certificates"),
        ("*.key", "private key files"),
        ("id_rsa", "default SSH private keys"),
    ];

    let mut missing: Vec<String> = Vec::new();
    for (pattern, desc) in want {
        let covered = content.lines().any(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return false;
            }
            line == pattern
                || line == format!("/{pattern}")
                || line.ends_with(&format!("/{pattern}"))
                || (pattern.starts_with("*.") && line.contains(pattern))
                || (pattern == ".env" && (line == ".env*" || line == "*.env"))
        });
        if !covered {
            missing.push(format!("`{pattern}` ({desc})"));
        }
    }

    if missing.is_empty() {
        findings.push(SecurityFinding {
            category: "Repository Hygiene",
            severity: Severity::Info,
            title: ".gitignore covers common secret-file patterns".into(),
            description: ".gitignore covers .env, *.pem, *.key, and id_rsa-style files.".into(),
            status: FindingStatus::Pass,
            remediation: None,
        });
    } else {
        findings.push(SecurityFinding {
            category: "Repository Hygiene",
            severity: Severity::Medium,
            title: ".gitignore is missing common secret-file patterns".into(),
            description: format!(
                "These patterns are not covered by .gitignore: {}.",
                missing.join(", ")
            ),
            status: FindingStatus::Warning,
            remediation: Some(
                "Add the missing patterns to .gitignore so secret-bearing files cannot be committed by mistake.".into(),
            ),
        });
    }
}

fn scan_security_disclosure_policy(root: &str, findings: &mut Vec<SecurityFinding>) {
    let candidates = ["SECURITY.md", ".github/SECURITY.md", "docs/SECURITY.md"];
    let found = candidates.iter().any(|c| Path::new(root).join(c).exists());

    findings.push(SecurityFinding {
        category: "Repository Hygiene",
        severity: Severity::Info,
        title: "SECURITY.md disclosure policy".into(),
        description: if found {
            "A SECURITY.md is published at one of SECURITY.md, .github/SECURITY.md, or docs/SECURITY.md.".into()
        } else {
            "No SECURITY.md was found at the repository root, in .github/, or in docs/. Disclosure policy is not published.".into()
        },
        status: if found { FindingStatus::Pass } else { FindingStatus::Warning },
        remediation: if found { None } else {
            Some("Publish a SECURITY.md describing how to report vulnerabilities and what response to expect.".into())
        },
    });
}

// ── Per-path checks ─────────────────────────────────────────────────────────

/// Returns true if the prefix of `s` contains at least `min_len` consecutive
/// characters that satisfy `predicate`. Used to validate that a token-shaped
/// pattern is followed by a long-enough run of valid token characters before
/// the next delimiter.
fn run_of_class<F: Fn(char) -> bool>(s: &str, min_len: usize, predicate: F) -> bool {
    let mut count = 0;
    for c in s.chars() {
        if predicate(c) {
            count += 1;
            if count >= min_len {
                return true;
            }
        } else {
            return false;
        }
    }
    count >= min_len
}

fn scan_secrets_in(path: &str, source: &str, findings: &mut Vec<SecurityFinding>) {
    type Check = fn(&str) -> bool;
    let patterns: &[(&str, &str, Check)] = &[
        ("AWS access key id", "AKIA", |after| {
            run_of_class(after, 16, |c| c.is_ascii_uppercase() || c.is_ascii_digit())
        }),
        ("GitHub personal access token", "ghp_", |after| {
            run_of_class(after, 36, |c| c.is_ascii_alphanumeric())
        }),
        ("GitHub fine-grained token", "github_pat_", |after| {
            run_of_class(after, 22, |c| c.is_ascii_alphanumeric() || c == '_')
        }),
        ("Anthropic API key", "sk-ant-", |after| {
            run_of_class(after, 30, |c| {
                c.is_ascii_alphanumeric() || c == '-' || c == '_'
            })
        }),
        ("OpenAI API key", "sk-", |after| {
            !after.starts_with("ant-")
                && run_of_class(after, 40, |c| {
                    c.is_ascii_alphanumeric() || c == '_' || c == '-'
                })
        }),
        ("Stripe live secret key", "sk_live_", |after| {
            run_of_class(after, 24, |c| c.is_ascii_alphanumeric())
        }),
        ("Slack token", "xoxb-", |after| {
            run_of_class(after, 10, |c| c.is_ascii_alphanumeric() || c == '-')
        }),
        ("Private key block", "-----BEGIN ", |after| {
            after.starts_with("RSA PRIVATE KEY-----")
                || after.starts_with("OPENSSH PRIVATE KEY-----")
                || after.starts_with("EC PRIVATE KEY-----")
                || after.starts_with("DSA PRIVATE KEY-----")
                || after.starts_with("PRIVATE KEY-----")
        }),
    ];

    for (label, prefix, check) in patterns {
        let mut cursor = 0;
        while let Some(idx) = source[cursor..].find(prefix) {
            let absolute = cursor + idx;
            let after_prefix = &source[absolute + prefix.len()..];
            if check(after_prefix) {
                findings.push(SecurityFinding {
                    category: "Secrets",
                    severity: Severity::Critical,
                    title: format!("Possible {label} in {path}"),
                    description: format!(
                        "A string matching the {label} pattern was found in `{path}`. Even commented-out or example secrets should not be committed."
                    ),
                    status: FindingStatus::Fail,
                    remediation: Some(
                        "Move the value to environment variables or a secrets manager. Once committed, treat the credential as compromised and rotate it.".into(),
                    ),
                });
                break;
            }
            cursor = absolute + prefix.len();
        }
    }
}

fn scan_weak_crypto_in(path: &str, source: &str, findings: &mut Vec<SecurityFinding>) {
    let weak: &[(&str, &[&str])] = &[
        ("MD5", &["Md5", "MD5", "md5::", "hashlib.md5"]),
        ("SHA-1", &["Sha1", "SHA1", "sha1::", "hashlib.sha1"]),
        ("DES", &["Des::", "crypto/des"]),
        ("RC4", &["Rc4", "RC4"]),
    ];

    for (label, needles) in weak {
        if !needles.iter().any(|n| source.contains(n)) {
            continue;
        }
        findings.push(SecurityFinding {
            category: "Cryptographic Hygiene",
            severity: Severity::Medium,
            title: format!("Weak primitive {label} referenced in {path}"),
            description: format!(
                "`{path}` references {label}. {label} is unsuitable for new cryptographic uses; verify each occurrence is a non-security checksum and not a security-relevant primitive."
            ),
            status: FindingStatus::Warning,
            remediation: Some(
                "For security-relevant uses, replace with SHA-256 or stronger. For non-security uses (e.g. cache keys), document the intent inline so reviewers don't have to re-derive it.".into(),
            ),
        });
    }
}

fn scan_plaintext_http_in(path: &str, source: &str, findings: &mut Vec<SecurityFinding>) {
    let allowed = [
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "::1",
        "example.com",
        "example.org",
        "example.net",
    ];
    let mut cursor = 0;
    let mut hits = 0usize;
    while let Some(idx) = source[cursor..].find("http://") {
        let absolute = cursor + idx;
        let after = &source[absolute + "http://".len()..];
        let host_end = after
            .find(|c: char| {
                c == '/' || c == '"' || c == '\'' || c == ' ' || c == ')' || c == '\n' || c == '\r'
            })
            .unwrap_or(after.len());
        let host = &after[..host_end];
        let host_no_port = host.split(':').next().unwrap_or(host);
        let is_allowed = allowed.contains(&host_no_port);
        if !is_allowed {
            hits += 1;
        }
        cursor = absolute + "http://".len();
    }

    if hits > 0 {
        findings.push(SecurityFinding {
            category: "Transport",
            severity: Severity::Low,
            title: format!("Plaintext http:// URL referenced in {path}"),
            description: format!(
                "`{path}` references {hits} non-localhost http:// URL(s). Plaintext transport allows on-path tampering and observation."
            ),
            status: FindingStatus::Warning,
            remediation: Some(
                "Use https:// where the destination supports it. If plaintext is required (e.g. internal-only mesh), document the reason inline.".into(),
            ),
        });
    }
}

// ── Export ───────────────────────────────────────────────────────────────────

pub fn export_findings_json(root: &str, findings: &[SecurityFinding]) -> Option<String> {
    let entries: Vec<serde_json::Value> = findings
        .iter()
        .map(|f| {
            serde_json::json!({
                "category": f.category,
                "severity": f.severity.to_string(),
                "status": f.status.to_string(),
                "title": f.title,
                "description": f.description,
                "remediation": f.remediation,
            })
        })
        .collect();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();

    let report = serde_json::json!({
        "timestamp_unix": timestamp,
        "total_findings": findings.len(),
        "summary": {
            "critical": findings.iter().filter(|f| f.severity == Severity::Critical).count(),
            "high": findings.iter().filter(|f| f.severity == Severity::High).count(),
            "medium": findings.iter().filter(|f| f.severity == Severity::Medium).count(),
            "low": findings.iter().filter(|f| f.severity == Severity::Low).count(),
            "info": findings.iter().filter(|f| f.severity == Severity::Info).count(),
            "pass": findings.iter().filter(|f| f.status == FindingStatus::Pass).count(),
            "fail": findings.iter().filter(|f| f.status == FindingStatus::Fail).count(),
            "warn": findings.iter().filter(|f| f.status == FindingStatus::Warning).count(),
        },
        "findings": entries,
    });

    let json = serde_json::to_string_pretty(&report).ok()?;
    let path = Path::new(root).join("security-review.json");
    std::fs::write(&path, &json).ok()?;
    Some(path.to_string_lossy().to_string())
}

// ── UI Component ────────────────────────────────────────────────────────────

/// Owned category group for rendering.
struct CategoryGroup {
    name: String,
    items: Vec<SecurityFinding>,
}

#[component]
pub fn SecurityPanel(findings: Signal<Vec<SecurityFinding>>, root: Signal<String>) -> Element {
    let findings_read = findings.read();

    if findings_read.is_empty() {
        return rsx! {
            div { class: "editor-content",
                div { class: "text-muted editor-empty", "Click \"Security Review\" to scan the project." }
            }
        };
    }

    let total = findings_read.len();
    let pass_count = findings_read
        .iter()
        .filter(|f| f.status == FindingStatus::Pass)
        .count();
    let fail_count = findings_read
        .iter()
        .filter(|f| f.status == FindingStatus::Fail)
        .count();
    let warn_count = findings_read
        .iter()
        .filter(|f| f.status == FindingStatus::Warning)
        .count();
    let critical_count = findings_read
        .iter()
        .filter(|f| f.severity == Severity::Critical)
        .count();
    let high_count = findings_read
        .iter()
        .filter(|f| f.severity == Severity::High)
        .count();

    // Group by category (clone into owned data so the borrow can end).
    let mut categories: Vec<CategoryGroup> = Vec::new();
    for f in findings_read.iter() {
        if let Some(group) = categories.iter_mut().find(|g| g.name == f.category) {
            group.items.push(f.clone());
        } else {
            categories.push(CategoryGroup {
                name: f.category.to_string(),
                items: vec![f.clone()],
            });
        }
    }

    let score_class = if fail_count > 0 || critical_count > 0 {
        "score-badge score-bad"
    } else if warn_count > 3 || high_count > 0 {
        "score-badge score-mixed"
    } else {
        "score-badge score-good"
    };
    let score_label = if fail_count > 0 || critical_count > 0 {
        "Needs Attention"
    } else if warn_count > 3 || high_count > 0 {
        "Fair"
    } else {
        "Good"
    };

    // Drop the read guard before entering rsx.
    drop(findings_read);

    rsx! {
        div { class: "editor-content security-panel",
            // Summary bar.
            div { class: "sec-summary",
                span { class: "{score_class}", "{score_label}" }
                span { class: "sec-stat", "{total} checks" }
                if pass_count > 0 {
                    span { class: "sec-stat status-pass", "{pass_count} pass" }
                }
                if fail_count > 0 {
                    span { class: "sec-stat status-fail", "{fail_count} fail" }
                }
                if warn_count > 0 {
                    span { class: "sec-stat status-warn", "{warn_count} warn" }
                }
                button {
                    class: "btn btn-xs sec-export",
                    onclick: move |_| {
                        let r = root.read().clone();
                        let f = findings.read();
                        if let Some(path) = export_findings_json(&r, &f) {
                            tracing::info!("Exported security report to {path}");
                        }
                    },
                    "Export JSON"
                }
            }

            // Category sections.
            for group in categories {
                div { class: "sec-category",
                    div { class: "sec-category-header", "{group.name}" }
                    for finding in group.items {
                        div { class: "sec-finding",
                            div { class: "sec-finding-header",
                                span { class: "sec-sev-badge {finding.severity.css_class()}", "{finding.severity}" }
                                span { class: "sec-status-badge {finding.status.css_class()}", "{finding.status}" }
                                span { class: "sec-finding-title", "{finding.title}" }
                            }
                            div { class: "sec-finding-desc", "{finding.description}" }
                            if let Some(ref rem) = finding.remediation {
                                div { class: "sec-remediation",
                                    span { class: "sec-remediation-label", "Remediation: " }
                                    "{rem}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_display() {
        assert_eq!(Severity::Critical.to_string(), "CRITICAL");
        assert_eq!(Severity::High.to_string(), "HIGH");
        assert_eq!(Severity::Medium.to_string(), "MEDIUM");
        assert_eq!(Severity::Low.to_string(), "LOW");
        assert_eq!(Severity::Info.to_string(), "INFO");
    }

    #[test]
    fn finding_status_display() {
        assert_eq!(FindingStatus::Pass.to_string(), "PASS");
        assert_eq!(FindingStatus::Fail.to_string(), "FAIL");
        assert_eq!(FindingStatus::Warning.to_string(), "WARN");
    }

    #[test]
    fn scan_produces_findings_for_real_project() {
        let root = std::env::var("CARGO_MANIFEST_DIR")
            .map(|d| {
                std::path::PathBuf::from(d)
                    .join("../..")
                    .to_string_lossy()
                    .to_string()
            })
            .unwrap_or_else(|_| ".".into());
        let targets = crate::agent::types::ScanTargets::default();
        let findings = run_security_scan(&root, &targets);
        assert!(!findings.is_empty(), "expected at least one finding");
    }

    #[test]
    fn empty_targets_warns_about_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let targets = crate::agent::types::ScanTargets::default();
        let findings = run_security_scan(dir.path().to_str().unwrap(), &targets);
        assert!(
            findings
                .iter()
                .any(|f| f.title.contains("No security-scan paths declared")),
            "expected the empty-paths configuration warning"
        );
        for f in &findings {
            assert!(
                f.status == FindingStatus::Fail || f.status == FindingStatus::Warning,
                "expected fail/warn for empty project, got {:?} for '{}'",
                f.status,
                f.title
            );
        }
    }

    #[test]
    fn findings_sorted_failures_first() {
        let dir = tempfile::tempdir().unwrap();
        let targets = crate::agent::types::ScanTargets::default();
        let findings = run_security_scan(dir.path().to_str().unwrap(), &targets);
        let mut saw_pass = false;
        for f in &findings {
            if f.status == FindingStatus::Pass {
                saw_pass = true;
            }
            if saw_pass {
                assert_ne!(
                    f.status,
                    FindingStatus::Fail,
                    "fail finding after pass: '{}'",
                    f.title
                );
            }
        }
    }

    fn write_file(root: &Path, rel: &str, content: &str) {
        let abs = root.join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(abs, content).unwrap();
    }

    #[test]
    fn scan_detects_aws_access_key() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(
            root,
            "src/leak.rs",
            "let creds = (\"AKIAIOSFODNN7EXAMPLE\", \"secret\");\n",
        );
        let targets = crate::agent::types::ScanTargets {
            paths: vec!["src/leak.rs".into()],
        };
        let findings = run_security_scan(root.to_str().unwrap(), &targets);
        assert!(
            findings
                .iter()
                .any(|f| f.category == "Secrets" && f.title.contains("AWS access key id")),
            "expected an AWS access-key-id finding"
        );
    }

    #[test]
    fn scan_detects_private_key_block() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(
            root,
            "src/keys.rs",
            "static KEY: &str = \"-----BEGIN RSA PRIVATE KEY-----\\nMIIE...\\n-----END RSA PRIVATE KEY-----\";\n",
        );
        let targets = crate::agent::types::ScanTargets {
            paths: vec!["src/keys.rs".into()],
        };
        let findings = run_security_scan(root.to_str().unwrap(), &targets);
        assert!(
            findings
                .iter()
                .any(|f| f.category == "Secrets" && f.title.contains("Private key block")),
            "expected a private-key-block finding"
        );
    }

    #[test]
    fn scan_flags_weak_crypto() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(
            root,
            "src/hash.rs",
            "use md5::Md5;\nfn h() -> Md5 { todo!() }\n",
        );
        let targets = crate::agent::types::ScanTargets {
            paths: vec!["src/hash.rs".into()],
        };
        let findings = run_security_scan(root.to_str().unwrap(), &targets);
        assert!(
            findings
                .iter()
                .any(|f| f.category == "Cryptographic Hygiene" && f.title.contains("MD5")),
            "expected an MD5 weak-primitive finding"
        );
    }

    #[test]
    fn scan_flags_plaintext_http() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(
            root,
            "src/client.rs",
            "const ENDPOINT: &str = \"http://api.example-real.test/v1\";\n",
        );
        let targets = crate::agent::types::ScanTargets {
            paths: vec!["src/client.rs".into()],
        };
        let findings = run_security_scan(root.to_str().unwrap(), &targets);
        assert!(
            findings
                .iter()
                .any(|f| f.category == "Transport" && f.title.contains("Plaintext")),
            "expected a plaintext-http finding"
        );
    }

    #[test]
    fn scan_skips_localhost_http() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(
            root,
            "src/local.rs",
            "const LOCAL: &str = \"http://localhost:8080/healthz\";\n",
        );
        let targets = crate::agent::types::ScanTargets {
            paths: vec!["src/local.rs".into()],
        };
        let findings = run_security_scan(root.to_str().unwrap(), &targets);
        assert!(
            !findings.iter().any(|f| f.category == "Transport"),
            "expected no transport finding for localhost URLs"
        );
    }

    #[test]
    fn scan_warns_on_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let targets = crate::agent::types::ScanTargets {
            paths: vec!["does/not/exist.rs".into()],
        };
        let findings = run_security_scan(dir.path().to_str().unwrap(), &targets);
        assert!(
            findings
                .iter()
                .any(|f| f.title.contains("Declared scan path not found")),
            "expected a missing-path configuration warning"
        );
    }

    #[test]
    fn scan_passes_on_complete_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_file(root, ".gitignore", ".env\n*.pem\n*.key\nid_rsa\n");
        let targets = crate::agent::types::ScanTargets::default();
        let findings = run_security_scan(root.to_str().unwrap(), &targets);
        assert!(
            findings.iter().any(|f| f.category == "Repository Hygiene"
                && f.title.contains(".gitignore covers")
                && f.status == FindingStatus::Pass),
            "expected a passing gitignore-coverage finding"
        );
    }

    #[test]
    fn export_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let findings = vec![SecurityFinding {
            category: "Test",
            severity: Severity::Info,
            title: "Test finding".into(),
            description: "Test description".into(),
            status: FindingStatus::Pass,
            remediation: None,
        }];
        let path = export_findings_json(dir.path().to_str().unwrap(), &findings);
        assert!(path.is_some());
        let content = std::fs::read_to_string(path.unwrap()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(v["total_findings"], 1);
        assert_eq!(v["findings"][0]["title"], "Test finding");
    }
}
