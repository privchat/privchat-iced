# PrivChat Desktop (Iced)

跨平台桌面客户端，基于 [Iced](https://github.com/iced-rs/iced) GUI 框架。

## 前置条件

- Rust 1.75+
- `privchat-sdk` 和 `privchat-protocol` 已编译通过（通过 workspace 路径依赖引入）

## 开发运行

```bash
# 默认使用 config/local.toml 配置
cargo run --release
```

## 多环境配置

通过 `PRIVCHAT_PROFILE` 环境变量选择配置文件：

```bash
# 使用 config/loan.toml 配置运行
PRIVCHAT_PROFILE=loan cargo run --release

# 使用 config/prod.toml 配置运行
PRIVCHAT_PROFILE=prod cargo run --release
```

可用配置文件位于 `config/` 目录：

| Profile | 配置文件 | 说明 |
|---------|----------|------|
| `local` | `config/local.toml` | 本地开发（默认） |
| `loan` | `config/loan.toml` | 测试环境 |
| `prod` | `config/prod.toml` | 生产环境 |
| `live` | `config/live.toml` | 线上环境 |
| `dubai` | `config/dubai.toml` | 海外环境 |

## 打包发布

```bash
# 使用 cargo-bundle 打包为 macOS .app
# 需要先安装: cargo install cargo-bundle
PRIVCHAT_PROFILE=loan cargo bundle --release
```

打包产物位于 `target/release/bundle/osx/PrivChat.app`。

## 架构

```
src/
  main.rs                          # 应用入口、窗口管理
  config.rs                        # 配置文件加载与校验
  app/                             # 状态管理 + UDF 更新循环
    mod.rs
    message.rs                     # AppMessage 枚举
    state.rs                       # AppState 状态树
    update.rs                      # update() — 唯一状态变更入口
    subscription.rs                # Iced Subscription 装配
    route.rs                       # 路由枚举
    reporting.rs                   # 遥测/指标
  sdk/                             # 薄 SDK 桥接层
    mod.rs
    bridge.rs                      # SdkBridge trait + PrivchatSdkBridge
    events.rs                      # SDK 事件 -> AppMessage 映射
  presentation/                    # 视图模型 + 适配层
    mod.rs
    vm.rs                          # UI ViewModels
    adapter.rs                     # SDK Stored -> VM 适配
  ui/                              # 纯渲染层
    mod.rs
    screens/                       # 屏幕组合
      chat.rs
      session_list.rs
      settings.rs
      add_friend.rs
    widgets/                       # 无状态组件
      timeline_list.rs
      message_bubble.rs
      composer.rs
      unread_banner.rs
```

### 依赖方向

```
ui -> app -> sdk
      |
      -> presentation
sdk/events -> app
```

- `presentation::*` 不依赖 `app::*` / `ui::*`
- `app::*` 不依赖 `ui::*` / `privchat-sdk`
- `ui::*` 不依赖 `sdk::*` / `privchat-sdk`
- `privchat-sdk` 仅在 `sdk::bridge` 中导入

## 关键设计

- `update()` 是唯一状态变更入口
- Widget 不直接调用 SDK
- `sdk::bridge` 是唯一导入 `privchat-sdk` 的模块
- 用户信息通过 `get_user_by_id` 获取，自带本地缓存 + 远程回源 + 写缓存
- 文件/图片下载通过 RPC `file/get_url` 获取临时 URL，客户端不再拼装 URL
