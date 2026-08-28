use std::collections::HashMap;
use std::sync::Once;

static APPLIED: Once = Once::new();

/// Extract and apply behavioral overrides from X-OpenAB-* headers.
///
/// The multitenant Router injects these headers on ECS-bound forwards:
/// - `X-OpenAB-System-Prompt`: base64-encoded UTF-8 system prompt
/// - `X-OpenAB-Tool-Display`: plain string (full/compact/none)
/// - `X-OpenAB-Env`: base64-encoded JSON map of env vars
///
/// Applied once on first webhook; subsequent calls are no-ops.
pub fn apply_from_headers(headers: &axum::http::HeaderMap) {
    if !has_any_header(headers) {
        return;
    }
    APPLIED.call_once(|| apply_inner(headers));
}

fn has_any_header(h: &axum::http::HeaderMap) -> bool {
    h.contains_key("x-openab-system-prompt")
        || h.contains_key("x-openab-tool-display")
        || h.contains_key("x-openab-env")
}

fn apply_inner(headers: &axum::http::HeaderMap) {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;

    if let Some(val) = headers.get("x-openab-env").and_then(|v| v.to_str().ok()) {
        match b64.decode(val) {
            Ok(bytes) => match serde_json::from_slice::<HashMap<String, String>>(&bytes) {
                Ok(env_map) => {
                    let keys: Vec<&str> = env_map.keys().map(|k| k.as_str()).collect();
                    for (k, v) in &env_map {
                        std::env::set_var(k, v);
                    }
                    // Publish the key list so openab-core can inherit them
                    // into agent spawn env without a direct crate dependency.
                    std::env::set_var("OPENAB_BEHAVIORAL_ENV_KEYS", keys.join(","));
                    tracing::info!(
                        keys = ?keys,
                        "behavioral: applied env overrides from X-OpenAB-Env"
                    );
                }
                Err(e) => tracing::warn!("behavioral: X-OpenAB-Env JSON decode failed: {e}"),
            },
            Err(e) => tracing::warn!("behavioral: X-OpenAB-Env base64 decode failed: {e}"),
        }
    }

    if let Some(val) = headers
        .get("x-openab-system-prompt")
        .and_then(|v| v.to_str().ok())
    {
        match b64.decode(val) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(prompt) => write_system_prompt(&prompt),
                Err(e) => {
                    tracing::warn!("behavioral: X-OpenAB-System-Prompt UTF-8 error: {e}")
                }
            },
            Err(e) => {
                tracing::warn!("behavioral: X-OpenAB-System-Prompt base64 decode failed: {e}")
            }
        }
    }

    if let Some(val) = headers
        .get("x-openab-tool-display")
        .and_then(|v| v.to_str().ok())
    {
        std::env::set_var("OPENAB_TOOL_DISPLAY", val);
        tracing::info!(tool_display = val, "behavioral: applied tool_display");
    }
}

const BEHAVIORAL_MARKER: &str = "<!-- openab:behavioral-profile -->";

/// Append the profile persona to the seeded CLAUDE.md instead of replacing
/// it, so deployment-level instructions (e.g. reply-format rules from the
/// pre_seed base layer) survive. A marker keeps the write idempotent: any
/// previous behavioral section is replaced, not stacked.
fn write_system_prompt(prompt: &str) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/node".into());
    let dir = format!("{home}/.claude");
    let path = format!("{dir}/CLAUDE.md");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(path = %dir, error = %e, "behavioral: failed to create prompt dir");
        return;
    }
    let base = std::fs::read_to_string(&path).unwrap_or_default();
    let base = match base.find(BEHAVIORAL_MARKER) {
        Some(idx) => base[..idx].trim_end().to_string(),
        None => base.trim_end().to_string(),
    };
    let combined = if base.is_empty() {
        format!("{BEHAVIORAL_MARKER}\n{prompt}\n")
    } else {
        format!("{base}\n\n{BEHAVIORAL_MARKER}\n{prompt}\n")
    };
    match std::fs::write(&path, combined) {
        Ok(()) => tracing::info!(path, chars = prompt.len(), "behavioral: appended system prompt"),
        Err(e) => tracing::warn!(path, error = %e, "behavioral: failed to write system prompt"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn has_any_header_detects_openab_headers() {
        let mut h = HeaderMap::new();
        assert!(!has_any_header(&h));
        h.insert("x-openab-env", "test".parse().unwrap());
        assert!(has_any_header(&h));
    }

    #[test]
    fn write_system_prompt_appends_to_base_and_is_idempotent() {
        let tmp = std::env::temp_dir().join(format!("oab-bhv-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join(".claude")).unwrap();
        std::fs::write(tmp.join(".claude/CLAUDE.md"), "## Base format rules\n").unwrap();
        // SAFETY: test-local HOME override; tests in this module run serially
        // for this var and nothing else in-process reads HOME concurrently.
        std::env::set_var("HOME", &tmp);
        write_system_prompt("Persona v1");
        write_system_prompt("Persona v2");
        let out = std::fs::read_to_string(tmp.join(".claude/CLAUDE.md")).unwrap();
        assert!(out.starts_with("## Base format rules"), "base must survive: {out}");
        assert!(out.contains("Persona v2"));
        assert!(!out.contains("Persona v1"), "old section must be replaced: {out}");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
