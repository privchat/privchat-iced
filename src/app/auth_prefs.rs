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

/// Device-id contract: a random UUID v4 (RFC 4122 variant), lowercase hyphenated.
/// The servers reject anything else (uppercase, unhyphenated, v1/v5, free-form strings).
pub fn is_canonical_device_id(value: &str) -> bool {
    match uuid::Uuid::parse_str(value) {
        Ok(id) => {
            id.get_version_num() == 4
                && id.get_variant() == uuid::Variant::RFC4122
                && id.hyphenated().to_string() == value
        }
        Err(_) => false,
    }
}

fn device_id_from_payload(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("device_id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| is_canonical_device_id(value))
        .map(ToOwned::to_owned)
}

/// Persisted device id: reuse the stored canonical v4, otherwise (missing or invalid)
/// generate a fresh random v4 once and persist it for every later login/refresh/connect.
pub fn load_or_create_device_id() -> String {
    let mut payload = load_payload();
    if let Some(device_id) = device_id_from_payload(&payload) {
        return device_id;
    }
    let device_id = uuid::Uuid::new_v4().to_string();
    payload["device_id"] = serde_json::Value::String(device_id.clone());
    save_payload(&payload);
    device_id
}

/// Persist the device id that a successful login actually used (e.g. a manually entered one).
pub fn save_device_id(device_id: &str) {
    if !is_canonical_device_id(device_id) {
        return;
    }
    let mut payload = load_payload();
    if device_id_from_payload(&payload).as_deref() == Some(device_id) {
        return;
    }
    payload["device_id"] = serde_json::Value::String(device_id.to_string());
    save_payload(&payload);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_device_id_accepts_only_lowercase_v4() {
        let v4 = uuid::Uuid::new_v4().to_string();
        assert!(is_canonical_device_id(&v4));
        assert!(!is_canonical_device_id(&v4.to_uppercase()));
        assert!(!is_canonical_device_id(&v4.replace('-', "")));
        assert!(!is_canonical_device_id(""));
        assert!(!is_canonical_device_id("dev-1"));
        assert!(!is_canonical_device_id("6ba7b810-9dad-11d1-80b4-00c04fd430c8"));
    }

    #[test]
    fn stored_device_id_is_used_only_when_canonical() {
        let v4 = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            device_id_from_payload(&serde_json::json!({ "device_id": v4 })),
            Some(v4.clone())
        );
        assert_eq!(
            device_id_from_payload(&serde_json::json!({ "device_id": v4.to_uppercase() })),
            None
        );
        assert_eq!(device_id_from_payload(&serde_json::json!({ "device_id": "dev-1" })), None);
        assert_eq!(device_id_from_payload(&serde_json::json!({})), None);
    }
}
