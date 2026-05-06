use serde::Deserialize;
use std::env;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub application: ApplicationConfig,
    pub network: NetworkConfig,
    pub servers: Vec<ServerConfig>,
    /// 账号体系归属（与 privchat-server `[account] mode` 对齐）；缺省视为 BUILTIN，
    /// 与 privchat-iced 历史行为兼容。详见 spec/02-server/AUTH_SPEC.md §1.1。
    #[serde(default)]
    pub account: AccountConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApplicationConfig {
    pub name: String,
}

/// 账号体系归属。BUILTIN = privchat-server 内置账号；PLATFORM = privchat-application
/// 平台账号（手机号 + 短信码）。
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum AccountMode {
    #[default]
    Builtin,
    Platform,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccountConfig {
    #[serde(default)]
    pub mode: AccountMode,
    /// PLATFORM 模式必填：privchat-application 路由组根 URL（含 `/app` 前缀，无尾斜杠）。
    /// e.g. `http://192.168.1.7:8080/app`。BUILTIN 模式忽略本字段。
    #[serde(default)]
    pub platform_base_url: Option<String>,
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
        .unwrap_or_else(|| "local".to_string())
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

    if matches!(config.account.mode, AccountMode::Platform) {
        let url = config
            .account
            .platform_base_url
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        if url.is_none() {
            anyhow::bail!(
                "profile={profile}: account.mode=PLATFORM requires non-empty account.platform_base_url"
            );
        }
    }

    Ok(())
}
