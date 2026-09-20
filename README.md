# ZeroApiKey

A local-first secret manager for macOS — desktop app **and** CLI — that replaces plaintext API keys and passwords scattered across ini files. Two interchangeable backends: an encrypted local vault, and [Infisical Cloud](https://infisical.com). Entries sync in both directions.

- **ZeroApiKey** — Tauri 2 + Preact desktop app with Touch ID unlock
- **zak** — full-featured CLI (same Rust crate, same data)

## Screenshots

| Main window | Add entry | Settings |
| --- | --- | --- |
| ![Main window](docs/screenshots/main.png) | ![Add entry dialog](docs/screenshots/add-entry.png) | ![Settings](docs/screenshots/settings.png) |

## Features

- Groups (e.g. `openai`, `kimi`) containing typed entries: `api-key`, `login`, `note`
- Values are masked by default; reveal for 5 seconds or copy straight to the clipboard
- Optional environment-variable name per entry; one-click export as `export ENV=...` shell statements
- Tolerant ini import with per-entry preview and selection
- Bidirectional sync between the local vault and Infisical Cloud
- English / Chinese UI (Settings → Language), persisted locally
- Desktop app and CLI operate on the same vault and config

## Architecture

```
zak (CLI) / ZeroApiKey (Tauri desktop)
  │
  ├── Store trait ─────────────────────────────┐
  │                                             │
  ├── LocalStore                      CloudStore
  │  ~/.zero-api-key/vault.enc         │  HTTPS REST API
  │  XChaCha20-Poly1305                ▼
  │  master key in macOS Keychain   Infisical Cloud
  │  gated by Touch ID              Project → Environment → Folder (group) → Secret (entry)
  │
  └── sync: local ⇄ cloud, per-conflict interactive resolution
```

Unified data model shared by both backends:

```
Group (e.g. openai / deepseek / cursor)
└── Entry
      key        UPPER_SNAKE (API_KEY / USERNAME / PASSWORD / BASE_URL / NOTE / custom)
      value      stored encrypted
      type       api-key | login | note     (cloud: secretMetadata)
      env_name?  linked environment variable (cloud: secretMetadata)
```

Mapping: local Group → cloud Folder; local Entry → Secret inside that folder.

## Security model

- The local vault (`~/.zero-api-key/vault.enc`) is JSON serialized, then encrypted with XChaCha20-Poly1305.
- The 32-byte random master key is generated on first use and stored in the macOS Keychain (service `zero-api-key`, item `local_vault_key_bio`).
- Before the master key is read, the app verifies the device owner through LocalAuthentication (Touch ID, falling back to the device passcode). The verification is cached for the lifetime of the process.
- Plaintext only exists in memory; nothing is written to disk unencrypted.
- Cloud credentials (Client ID / Client Secret) are stored in the Keychain, never in the config file.

Honest limitations:

- The app is **self-signed** (local certificate `ZeroApiKey Local Dev`), not notarized. A stable signature keeps the Keychain from re-prompting on every launch, but Gatekeeper will still warn on first open of a downloaded build — right-click → Open. For personal builds this is expected; there is no Apple Developer identity behind this project.
- The Touch ID gate is a pre-access check at the application layer (LAContext), not a Keychain ACL binding. It raises the bar against casual access, but a determined attacker with control of the logged-in session should be considered out of scope. Keychain ACL binding requires a proper Developer ID signature with entitlements, which is why this approach was chosen.

## Install

Download `ZeroApiKey_<version>_aarch64.dmg` from the latest release (or the `dist/` output of a local build), drag `ZeroApiKey.app` to Applications, then right-click → Open on first launch.

Build from source (requires Rust, pnpm, and the Tauri prerequisites):

```sh
pnpm install
pnpm dist        # produces dist/ZeroApiKey.app and dist/ZeroApiKey_*_aarch64.dmg
```

The CLI is built alongside the app:

```sh
cargo build --release --manifest-path src-tauri/Cargo.toml --bin zak
# binary: src-tauri/target/release/zak
```

## CLI usage

```
zak init                              # interactive setup: local / cloud / both
zak add <group> [--local|--cloud]     # interactive add (api-key / login / note)
zak get <group> [--key N] [--reveal] [--local|--cloud]
zak list [group] [--local|--cloud]
zak rm <group> [--key N] [--local|--cloud]
zak export <group> [--local|--cloud]  # prints export ENV=..., safe to eval
zak import <file.ini> [--local|--cloud]  # tolerant parse, confirm per entry
zak sync [--to cloud|--to local]      # interactive direction if omitted
```

- `--local` / `--cloud` override the default backend; otherwise `default_mode` from `config.toml` is used.
- `zak sync` conflict policy: same key with different values → masked diff, choose keep source / keep target / skip; `--force` overwrites the target with the source.

## Cloud setup (one-time)

1. In the Infisical web UI, create a project named `zero-api-key`.
2. Project → Access Control → Machine Identities: create an identity, enable **Universal Auth**, note the Client ID / Client Secret, and grant it write access to the project.
3. Run `zak init` (or Settings in the desktop app) and enter the credentials.

## Tech stack

- `tauri` 2 — desktop shell (`src-tauri`; the `zero_api_key_lib` crate backs both the CLI and the Tauri commands)
- `preact` + `vite` + `pnpm` — frontend (`src/`)
- `clap` (derive) — CLI
- `reqwest` (blocking + rustls) — Infisical API
- `chacha20poly1305` + `rand` + `base64` — vault encryption
- `keyring` — Keychain storage for cloud credentials and the vault master key
- `serde` / `serde_json` / `toml` — models and config
- `anyhow`, `dirs`, `dialoguer`

## License

[MIT](LICENSE) © zjl1985
