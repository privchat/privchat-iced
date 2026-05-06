//! PLATFORM 模式下与 privchat-application 的 HTTP 对接（spec/02-server/AUTH_SPEC.md §3.5）。
//!
//! 与 privchat-app `PlatformAccountLoginImpl` 对齐：
//! - `POST {baseUrl}/auth/send-sms-code`
//! - `POST {baseUrl}/auth/sms-login`
//! - `POST {baseUrl}/auth/refresh-token`
//!
//! `baseUrl` 由 profile 配置 `account.platform_base_url` 提供（含 `/app` 路由组前缀，无尾斜杠）。
//! Response 统一包在 `PlatformEnvelope<T>` 里：`code != 0` 视为业务错误。

use serde::{Deserialize, Serialize};

use crate::presentation::vm::UiError;

/// SMS 登录场景码（spec MEMBER_AUTH §SmsScene.MEMBER_LOGIN）。
const SCENE_LOGIN: i32 = 1;

/// 内部凭证结构（与 privchat-app `AccountCredentials` 同构）。
pub struct PlatformCredentials {
    pub user_id: u64,
    pub access_token: String,
    pub refresh_token: String,
    pub device_id: String,
}

#[derive(Serialize)]
struct DeviceInfo<'a> {
    #[serde(rename = "deviceId")]
    device_id: &'a str,
}

#[derive(Serialize)]
struct SmsLoginRequest<'a> {
    mobile: &'a str,
    #[serde(rename = "smsCode")]
    sms_code: &'a str,
    device: DeviceInfo<'a>,
}

#[derive(Serialize)]
struct SendSmsRequest<'a> {
    mobile: &'a str,
    scene: i32,
}

#[derive(Serialize)]
struct RefreshRequest<'a> {
    #[serde(rename = "refreshToken")]
    refresh_token: &'a str,
    #[serde(rename = "deviceId")]
    device_id: &'a str,
}

#[derive(Deserialize)]
struct PlatformEnvelope<T> {
    code: i32,
    message: Option<String>,
    data: Option<T>,
}

#[derive(Deserialize)]
struct MemberLoginResponse {
    #[serde(rename = "userId")]
    user_id: i64,
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "deviceId", default)]
    device_id: String,
}

fn http_client() -> Result<reqwest::Client, UiError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| UiError::Unknown(format!("http client build: {e}")))
}

fn map_envelope_error<T>(envelope: PlatformEnvelope<T>) -> Result<T, UiError> {
    if envelope.code != 0 {
        let msg = envelope
            .message
            .unwrap_or_else(|| format!("code={}", envelope.code));
        return Err(UiError::Unknown(format!(
            "platform: code={} message={}",
            envelope.code, msg
        )));
    }
    envelope
        .data
        .ok_or_else(|| UiError::Unknown("platform: empty data".to_string()))
}

pub async fn send_sms_code(base_url: &str, mobile: String) -> Result<(), UiError> {
    let client = http_client()?;
    let resp = client
        .post(format!("{}/auth/send-sms-code", base_url.trim_end_matches('/')))
        .json(&SendSmsRequest {
            mobile: &mobile,
            scene: SCENE_LOGIN,
        })
        .send()
        .await
        .map_err(|e| UiError::Unknown(format!("send_sms_code request: {e}")))?
        .json::<PlatformEnvelope<serde_json::Value>>()
        .await
        .map_err(|e| UiError::Unknown(format!("send_sms_code decode: {e}")))?;

    if resp.code != 0 {
        let msg = resp
            .message
            .unwrap_or_else(|| format!("code={}", resp.code));
        return Err(UiError::Unknown(format!(
            "platform: code={} message={}",
            resp.code, msg
        )));
    }
    Ok(())
}

pub async fn sms_login(
    base_url: &str,
    mobile: String,
    sms_code: String,
    device_id: String,
) -> Result<PlatformCredentials, UiError> {
    let client = http_client()?;
    let resp = client
        .post(format!("{}/auth/sms-login", base_url.trim_end_matches('/')))
        .json(&SmsLoginRequest {
            mobile: &mobile,
            sms_code: &sms_code,
            device: DeviceInfo {
                device_id: &device_id,
            },
        })
        .send()
        .await
        .map_err(|e| UiError::Unknown(format!("sms_login request: {e}")))?
        .json::<PlatformEnvelope<MemberLoginResponse>>()
        .await
        .map_err(|e| UiError::Unknown(format!("sms_login decode: {e}")))?;

    let data = map_envelope_error(resp)?;
    let resolved_device_id = if data.device_id.is_empty() {
        device_id
    } else {
        data.device_id
    };
    Ok(PlatformCredentials {
        user_id: data.user_id as u64,
        access_token: data.access_token,
        refresh_token: data.refresh_token,
        device_id: resolved_device_id,
    })
}

pub async fn refresh_token(
    base_url: &str,
    refresh_token: String,
    device_id: String,
) -> Result<PlatformCredentials, UiError> {
    let client = http_client()?;
    let resp = client
        .post(format!(
            "{}/auth/refresh-token",
            base_url.trim_end_matches('/')
        ))
        .json(&RefreshRequest {
            refresh_token: &refresh_token,
            device_id: &device_id,
        })
        .send()
        .await
        .map_err(|e| UiError::Unknown(format!("refresh_token request: {e}")))?
        .json::<PlatformEnvelope<MemberLoginResponse>>()
        .await
        .map_err(|e| UiError::Unknown(format!("refresh_token decode: {e}")))?;

    let data = map_envelope_error(resp)?;
    let resolved_device_id = if data.device_id.is_empty() {
        device_id
    } else {
        data.device_id
    };
    Ok(PlatformCredentials {
        user_id: data.user_id as u64,
        access_token: data.access_token,
        refresh_token: data.refresh_token,
        device_id: resolved_device_id,
    })
}
