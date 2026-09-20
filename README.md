# zero-api-key

个人密钥管理 CLI（Rust），后端使用 [Infisical](https://github.com/infisical/infisical) Cloud，替代散落在 ini 文件里的明文密码和 API key。

## 架构

```
zak (Rust CLI)
  │  HTTPS REST API + Bearer token
  ▼
Infisical Cloud (app.infisical.com)
  └── Project: zero-api-key
        └── Environment: dev
              └── Folder（分组，如 openai / deepseek / cursor）
                    └── Secret 条目
```

- **分组 = Folder**：`POST /api/v2/folders`，每个服务一个目录（`openai`、`deepseek`、`cursor` …）
- **条目 = Secret**：目录下放 `API_KEY` / `USERNAME` / `PASSWORD` / `BASE_URL` / `NOTE` 等键
  - `POST /api/v4/secrets/{name}` 创建，`PATCH` 更新，`GET /api/v4/secrets` 列出
  - 额外信息走 `secretMetadata`（如 `type=login`、`env_name=OPENAI_API_KEY`）
- **认证**：Machine Identity + Universal Auth（client_id / client_secret）
  - `POST /api/v1/auth/universal-auth/login` 换短期 access token，缓存复用
  - client_id / client_secret 存 macOS 钥匙串（service: `zero-api-key`），不落盘明文

## 命令

```
zak init                          # 配置 project + 录入 machine identity 凭证（入钥匙串）
zak add <group>                   # 交互式添加条目（api-key / login / note）
zak get <group> [--key NAME] [--reveal]   # 默认掩码，--reveal 显示明文
zak list [group]                  # 列出分组 / 条目名
zak rm <group> [--key NAME]
zak export <group>                # 输出 export ENV=... 形式，可 eval
zak import <ini 文件>             # 容错解析旧 api_key.ini，逐条确认后上传
```

## 技术栈

- `clap`（derive）— CLI
- `reqwest`（blocking + rustls）— HTTPS 客户端
- `serde` / `serde_json` — API 模型
- `keyring` — macOS 钥匙串存凭证
- `anyhow` — 错误处理，分层风格参考 codex_clear
- `dirs` — 配置目录（`~/.zero-api-key/config.toml`，只存非敏感配置：API 地址、project id）

## 前置条件（一次性，在 Infisical 网页端操作）

1. 创建 Project：`zero-api-key`
2. Project → Access Control → Machine Identities 创建身份，启用 **Universal Auth**，拿到 Client ID / Client Secret
3. 给身份分配项目内 admin（或 developer）角色
4. `zak init` 时录入以上信息
