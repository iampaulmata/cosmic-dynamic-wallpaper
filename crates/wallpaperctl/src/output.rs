//! Human-readable vs. machine-readable (`--json`) output rendering, shared across
//! every data-returning command (FR-013).

use std::borrow::Cow;

use serde::Serialize;

/// Spec 011 US5 FR-018 (research.md R14): a pack's `name` field is untrusted (it
/// comes from a manifest a stranger may have authored) and `commands::list::packs`'s
/// human-readable output interpolates it directly into a tab-delimited line — a name
/// containing tab or newline characters can render as fabricated extra rows,
/// reproduced by the audit spoofing a fake output entry. Replaces `\t`/`\n`/`\r` with
/// a single space (collapsing consecutive control characters rather than leaving
/// visible run of spaces) so the rendered line always stays exactly one line, one
/// field per tab-stop, no matter what a pack's manifest declares.
///
/// Deliberately **not** applied to `--json` output — `serde_json`'s own string
/// escaping already makes tab/newline injection into the *document structure*
/// impossible, so `--json` continues to carry the raw value verbatim, matching every
/// other command's `--json`-carries-truth posture in this crate.
pub fn sanitize_for_tsv(s: &str) -> Cow<'_, str> {
    if !s.contains(['\t', '\n', '\r']) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut last_was_space = false;
    for c in s.chars() {
        if matches!(c, '\t' | '\n' | '\r') {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    Cow::Owned(out)
}

/// Render `value` as either a human-readable string (`human`, lazily built — only
/// called when actually needed) or compact JSON, depending on `json`.
///
/// JSON serialization failure is handled defensively (a JSON error object) rather than
/// panicking — every command path stays panic-free (constitution Principle VIII), even
/// though a `Serialize` failure on these plain-data types should never actually happen.
pub fn render<T: Serialize>(json: bool, value: &T, human: impl FnOnce() -> String) -> String {
    if json {
        serde_json::to_string(value)
            .unwrap_or_else(|e| format!(r#"{{"error":"failed to serialize output: {e}"}}"#))
    } else {
        human()
    }
}

/// A short success acknowledgement for commands with no return value beyond
/// success/failure (`register`, `remove`, `assign`, `location set|clear`,
/// `reevaluate`) — `{"ok": true}` under `--json` (contracts/wallpaperctl-cli.md).
#[derive(Debug, Serialize)]
pub struct Ack {
    pub ok: bool,
}

impl Ack {
    pub fn ok() -> Self {
        Self { ok: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_mode_calls_the_closure() {
        let out = render(false, &Ack::ok(), || "registered".to_string());
        assert_eq!(out, "registered");
    }

    #[test]
    fn json_mode_serializes_the_value() {
        let out = render(true, &Ack::ok(), || "registered".to_string());
        assert_eq!(out, r#"{"ok":true}"#);
    }

    /// Spec 011 US5 FR-018 (research.md R14) — the audit's exact reproduction shape: a
    /// pack `name` containing a tab and a newline must not be able to render as
    /// additional fake tab-delimited rows.
    #[test]
    fn sanitize_for_tsv_collapses_tabs_and_newlines() {
        assert_eq!(sanitize_for_tsv("evil\tDP-3\tknown\nfake-row"), "evil DP-3 known fake-row");
        assert_eq!(sanitize_for_tsv("a\r\nb"), "a b");
        assert_eq!(sanitize_for_tsv("plain name"), "plain name");
    }

    #[test]
    fn sanitize_for_tsv_leaves_ordinary_names_borrowed_not_reallocated() {
        // A `Cow::Borrowed` for the common case (no control characters) is itself part
        // of what this function promises — verified via the enum variant, not just
        // the resulting string value.
        assert!(matches!(sanitize_for_tsv("My Pack"), Cow::Borrowed(_)));
        assert!(matches!(sanitize_for_tsv("a\tb"), Cow::Owned(_)));
    }
}
