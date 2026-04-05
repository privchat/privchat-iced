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

pub fn load_last_username() -> Option<String> {
    let path = auth_prefs_path();
    let content = fs::read_to_string(path).ok()?;
    let parsed = serde_json::from_str::<serde_json::Value>(&content).ok()?;

    parsed
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

    let path = auth_prefs_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let payload = serde_json::json!({
        "last_username": username,
    });

    if let Ok(serialized) = serde_json::to_string_pretty(&payload) {
        let _ = fs::write(path, serialized);
    }
}
