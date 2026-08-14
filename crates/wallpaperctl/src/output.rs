//! Human-readable vs. machine-readable (`--json`) output rendering, shared across
//! every data-returning command (FR-013).

use serde::Serialize;

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
}
