pub(crate) fn strip_json_fence(s: &str) -> &str {
    s.trim().strip_prefix("```json")
     .and_then(|s| s.strip_suffix("```"))
     .map(str::trim)
     .unwrap_or(s)
}