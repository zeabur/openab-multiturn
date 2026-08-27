use std::collections::HashMap;

/// Read env overrides that the gateway's behavioral_overrides module
/// applied from X-OpenAB-Env headers. Keys are listed in the
/// OPENAB_BEHAVIORAL_ENV_KEYS process env var (comma-separated);
/// values are individual process env vars.
pub fn get_overrides() -> HashMap<String, String> {
    let keys = match std::env::var("OPENAB_BEHAVIORAL_ENV_KEYS") {
        Ok(v) if !v.is_empty() => v,
        _ => return HashMap::new(),
    };
    keys.split(',')
        .filter(|k| !k.is_empty())
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect()
}
