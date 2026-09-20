# zero-api-key

本地优先的个人密钥管理 CLI（Rust），灵感来自 [Infisical](https://github.com/infisical/infisical) 的项目/分组模型，但完全本地运行：一个加密的保险库文件 + macOS 钥匙串里的主密钥，替代散落在 ini 文件里的明文密码和 API key。

## 数据模型

```
Vault
└── Group（分组，如 openai / deepseek / cursor / 订阅）
    └── Secret（条目，三种类型）
        ├── ApiKey          { name, key, base_url?, env_name? }
        ├── UsernamePassword{ name, username, password, url? }
        └── Note            { name, value }      // token、订阅链接等
```

每个条目带 `tags`、`notes`、`created_at`、`updated_at`。

## 安全设计

- 保险库文件 `~/.zero-api-key/vault.json.enc`，XChaCha20-Poly1305 加密
- 32 字节随机主密钥存在 macOS 登录钥匙串（service: `zero-api-key`），日常使用不输密码
- 内存中解密即用即弃；`export` 只输出到 stdout，不写盘
- 参考 codex_clear 的路径安全校验模式，导入/导出路径做防穿越检查

## 命令规划

```
zak init                          # 初始化保险库（生成主密钥入钥匙串）
zak add <group> <name>            # 交互式添加（类型: api-key / login / note）
zak get <group>/<name> [--reveal] # 默认掩码显示，--reveal 显示明文
zak list [group]                  # 列出分组/条目（只显示名称，不显示值）
zak rm <group>/<name>
zak export <group>                # 输出 export ENV=... 形式，可 eval
zak import <file>                 # 导入旧的 api_key.ini（容错解析 + 人工确认）
```

## 技术栈

- `clap`（derive）— CLI
- `serde` / `serde_json` — 数据模型
- `chacha20poly1305` — 加密
- `keyring` — macOS 钥匙串存主密钥
- `anyhow` — 错误处理，分层风格参考 codex_clear
- `dirs` — 定位数据目录
