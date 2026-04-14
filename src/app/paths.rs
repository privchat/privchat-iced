use std::path::PathBuf;

/// 获取 SDK 存储根目录
/// 逻辑：优先 PRIVCHAT_DATA_DIR，否则 $HOME/.privchat-rust
pub fn get_sdk_storage_root() -> PathBuf {
    if let Some(value) = std::env::var("PRIVCHAT_DATA_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(value);
    }
    if let Some(home) = std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(home).join(".privchat-rust");
    }
    std::env::temp_dir().join("privchat-rust")
}

/// 统一计算媒体文件存储路径
/// 遵循 Spec: {root}/users/{uid}/files/{yyyymm}/{message_id}/
///
/// 参数说明：
/// - `message_id`: 必须使用**本地数据库消息 ID**（Local DB ID）。
///   注意：UI 上显示的可能为 server_message_id（几百），但文件目录是基于本地 ID（如 17, 18）创建的。
pub fn get_message_media_dir(uid: u64, message_id: u64, yyyymm: &str) -> PathBuf {
    let root = get_sdk_storage_root();
    root.join("users")
        .join(uid.to_string())
        .join("files")
        .join(yyyymm)
        .join(message_id.to_string())
}
