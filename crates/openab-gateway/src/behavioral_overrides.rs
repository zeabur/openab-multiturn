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

fn write_system_prompt(prompt: &str) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/node".into());
    let dir = format!("{home}/.claude");
    let path = format!("{dir}/CLAUDE.md");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(path = %dir, error = %e, "behavioral: failed to create prompt dir");
        return;
    }
    match std::fs::write(&path, prompt) {
        Ok(()) => tracing::info!(path, chars = prompt.len(), "behavioral: wrote system prompt"),
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
}
