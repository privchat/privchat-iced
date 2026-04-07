use serde::Deserialize;
use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub application: ApplicationConfig,
    pub network: NetworkConfig,
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApplicationConfig {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    pub connect_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub prefer_transport: Option<String>,
    pub fallback_transport: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub protocol: String,
    pub url: String,
    pub priority: u32,
}

fn resolve_config_dir() -> PathBuf {
    // 编译时引入源码目录的 config/ 路径
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_config = PathBuf::from(manifest_dir).join("config");
    if manifest_config.exists() {
        return manifest_config;
    }

    // 打包后从 exe 旁边找
    if let Ok(exe_path) = env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let parent_name = parent.file_name().map(|n| n.to_string_lossy());
            if parent_name.as_deref() == Some("MacOS") {
                if let Some(bundle_parent) = parent.parent() {
                    let resources = bundle_parent.join("Resources");
                    if resources.join("config").exists() {
                        return resources.join("config");
                    }
                }
            }
            return parent.join("config");
        }
    }
    manifest_config
}

pub fn load_app_config() -> anyhow::Result<(String, AppConfig)> {
    let profile = env::var("PRIVCHAT_PROFILE")
        .ok()
        .or_else(|| option_env!("PRIVCHAT_PROFILE").map(String::from))
        .unwrap_or_else(|| "local".to_string());
    let config_dir = resolve_config_dir();
    let path = config_dir.join(format!("{profile}.toml"));
    tracing::info!("loading config from: {}", path.display());
    let raw = fs::read_to_string(&path)?;
    let config: AppConfig = toml::from_str(&raw)?;
    validate_config(&profile, &config)?;
    Ok((profile, config))
}

fn validate_config(profile: &str, config: &AppConfig) -> anyhow::Result<()> {
    if config.servers.is_empty() {
        anyhow::bail!("profile={profile}: servers must not be empty");
    }

    for server in &config.servers {
        match server.protocol.as_str() {
            "quic" => {
                if !server.url.starts_with("quic://") {
                    anyhow::bail!("profile={profile}: protocol=quic but url is {}", server.url);
                }
            }
            "tcp" => {
                if !server.url.starts_with("tcp://") {
                    anyhow::bail!("profile={profile}: protocol=tcp but url is {}", server.url);
                }
            }
            other => {
                anyhow::bail!("profile={profile}: unsupported protocol={other}");
            }
        }
    }

    Ok(())
}
