use std::fs;
use std::path::PathBuf;

const AUTH_PREFS_FILE: &str = "auth_prefs.json";

fn auth_prefs_path() -> PathBuf {
    if let Some(data_dir) = std::env::var("PRIVCHAT_DATA_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(data_dir)
            .join("privchat-iced")
            .join(AUTH_PREFS_FILE);
    }

    if let Some(home_dir) = std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(home_dir)
            .join(".privchat")
            .join("privchat-iced")
            .join(AUTH_PREFS_FILE);
    }

    PathBuf::from(AUTH_PREFS_FILE)
}

fn load_payload() -> serde_json::Value {
    let path = auth_prefs_path();
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn save_payload(payload: &serde_json::Value) {
    let path = auth_prefs_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(serialized) = serde_json::to_string_pretty(payload) {
        let _ = fs::write(path, serialized);
    }
}

pub fn load_last_username() -> Option<String> {
    load_payload()
        .get("last_username")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|username| !username.is_empty())
        .map(ToOwned::to_owned)
}

pub fn save_last_username(username: &str) {
    let username = username.trim();
    if username.is_empty() {
        return;
    }
    let mut payload = load_payload();
    payload["last_username"] = serde_json::Value::String(username.to_string());
    save_payload(&payload);
}

/// PLATFORM 模式 host 持有 refresh_token。每个 user_id 独立存储。
/// BUILTIN 模式由 SDK 自管，本接口不参与。
pub fn save_refresh_token(user_id: u64, refresh_token: &str) {
    if refresh_token.trim().is_empty() {
        return;
    }
    let mut payload = load_payload();
    let map = payload
        .get_mut("refresh_tokens")
        .and_then(|v| v.as_object_mut());
    if let Some(map) = map {
        map.insert(
            user_id.to_string(),
            serde_json::Value::String(refresh_token.to_string()),
        );
    } else {
        payload["refresh_tokens"] = serde_json::json!({
            user_id.to_string(): refresh_token,
        });
    }
    save_payload(&payload);
}

pub fn load_refresh_token(user_id: u64) -> Option<String> {
    load_payload()
        .get("refresh_tokens")
        .and_then(|v| v.get(user_id.to_string()))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

pub fn clear_refresh_token(user_id: u64) {
    let mut payload = load_payload();
    if let Some(map) = payload
        .get_mut("refresh_tokens")
        .and_then(|v| v.as_object_mut())
    {
        map.remove(&user_id.to_string());
    }
    save_payload(&payload);
}
