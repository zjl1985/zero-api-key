# zero-api-key

个人密钥管理工具（Rust），**双后端**：本地加密保险库 + Infisical Cloud，两边可以互相同步。替代散落在 ini 文件里的明文密码和 API key。

- `zak`：CLI（`src-tauri` 下的 bin target）
- `ZeroApiKey`：Tauri 2 + Preact 桌面应用（`pnpm tauri dev` / `pnpm tauri build`），功能与 CLI 等价

## 架构

```
zak (Rust CLI)
  │
  ├── Store trait ─────────────────────────────┐
  │                                             │
  ├── LocalStore                      CloudStore
  │  ~/.zero-api-key/vault.enc         │  HTTPS REST API
  │  XChaCha20-Poly1305                ▼
  │  主密钥在 macOS 钥匙串        Infisical Cloud
  │                                  Project → Environment → Folder(分组) → Secret(条目)
  │
  └── sync：local ⇄ cloud 双向同步，冲突交互处理
```

## 统一数据模型（两个后端共用）

```
Group（分组，如 openai / deepseek / cursor）
└── Entry
      key        UPPER_SNAKE（API_KEY / USERNAME / PASSWORD / BASE_URL / NOTE / 自定义）
      value      密文存储
      type       api-key | login | note     （cloud 端存 secretMetadata）
      env_name?  对应的环境变量名             （cloud 端存 secretMetadata）
```

映射关系：本地 Group → Cloud Folder；本地 Entry → Cloud Folder 下的 Secret。

## 本地保险库

- 文件 `~/.zero-api-key/vault.enc`，JSON 序列化后 XChaCha20-Poly1305 加密
- 32 字节随机主密钥存 macOS 钥匙串（service `zero-api-key`，条目 `local_vault_key_bio`），首次使用自动生成
- 读取主密钥前经 LocalAuthentication 验证机主身份（Touch ID / 设备密码），进程内缓存一次验证
- 内存中解密即用；不落盘明文

## 云端（Infisical）

- Machine Identity + Universal Auth（client_id / client_secret 入钥匙串）
- 创建：`POST /api/v4/secrets/{name}`，列表：`GET /api/v4/secrets`，文件夹：`POST /api/v2/folders`
- type / env_name 走 `secretMetadata`

## 命令

```
zak init                          # 交互选择配置 local / cloud / 两者
zak add <group> [--local|--cloud] # 交互式添加（api-key / login / note）
zak get <group> [--key N] [--reveal] [--local|--cloud]
zak list [group] [--local|--cloud]
zak rm <group> [--key N] [--local|--cloud]
zak export <group> [--local|--cloud]   # 输出 export ENV=...，可 eval
zak import <ini 文件> [--local|--cloud] # 容错解析，逐条确认后写入当前后端
zak sync [--to cloud|--to local]       # 缺省交互选方向
```

- `--local` / `--cloud` 覆盖默认后端；未指定时用 config.toml 的 `default_mode`
- `zak sync` 冲突策略：同名条目值不同 → 展示掩码对比，选 保留源/保留目标/跳过；`--force` 直接用源覆盖目标

## 技术栈

- `tauri` 2 — 桌面壳（`src-tauri`，`zero_api_key_lib` 同时承载 CLI 与 Tauri commands）
- `preact` + `vite` + `pnpm` — 前端（`src/`，结构照搬 codex_clear）
- `clap`（derive）— CLI
- `reqwest`（blocking + rustls）— Cloud API
- `chacha20poly1305` + `rand` + `base64` — 本地保险库加密
- `keyring` — 钥匙串存云凭证 + 本地主密钥
- `serde` / `serde_json` / `toml` — 模型与配置
- `anyhow`、`dirs`、`dialoguer`
- 分层错误处理与模块划分参考 codex_clear

## 前置条件（云端模式一次性配置）

1. Infisical 网页端建 Project `zero-api-key`
2. Project → Access Control → Machine Identities 建身份，启用 **Universal Auth**，拿到 Client ID / Client Secret，给项目内写权限
3. `zak init` 时录入
