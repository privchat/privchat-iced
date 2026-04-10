use serde::Deserialize;
use std::env;

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

// 在编译时嵌入所有配置文件
const CONFIG_LOCAL: &str = include_str!("../config/local.toml");
const CONFIG_LOAN: &str = include_str!("../config/loan.toml");
const CONFIG_PROD: &str = include_str!("../config/prod.toml");
const CONFIG_LIVE: &str = include_str!("../config/live.toml");
const CONFIG_DUBAI: &str = include_str!("../config/dubai.toml");

fn get_embedded_config(profile: &str) -> Option<&'static str> {
    match profile {
        "local" => Some(CONFIG_LOCAL),
        "loan" => Some(CONFIG_LOAN),
        "prod" => Some(CONFIG_PROD),
        "live" => Some(CONFIG_LIVE),
        "dubai" => Some(CONFIG_DUBAI),
        _ => None,
    }
}

pub fn load_app_config() -> anyhow::Result<(String, AppConfig)> {
    let profile = env::var("PRIVCHAT_PROFILE")
        .ok()
        .or_else(|| option_env!("PRIVCHAT_PROFILE").map(String::from))
        .unwrap_or_else(|| "loan".to_string())
        .trim()
        .to_string();

    // 从编译时嵌入的配置中获取
    let config_content = get_embedded_config(&profile)
        .ok_or_else(|| anyhow::anyhow!("Unknown profile: '{}', supported profiles: local, loan, prod, live, dubai", profile))?;

    tracing::info!("loading embedded config for profile: {}", profile);

    let config: AppConfig = toml::from_str(config_content)?;
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
