# ZeroApiKey

A local-first secret manager for macOS, built with **Rust + GPUI**. A compact,
Zed-inspired desktop workspace and the `zak` CLI share the same encrypted vault
and Infisical Cloud backend.

The desktop app renders directly with Metal. It ships no WebView, JavaScript
runtime, web frontend, bundled fonts, or editor component framework.

## Desktop

- Three-pane workspace: groups, virtualized entry list, secret details.
- Instant indexed search across groups, keys, environment variables, and comments.
  Search uses masked metadata in memory and makes no network requests per keystroke.
- API key, login, and note entries; create, edit, rename, delete, reveal, and copy.
- Values hidden by default; revealed values disappear after five seconds.
- Local encrypted vault and Infisical Cloud, with bidirectional sync.
- INI import with masked preview and per-entry selection; shell export per group.
- App password and optional Touch ID, checked by a shared desktop service.
- Forest dark / paper light themes; English / Simplified Chinese preferences.
- Native text input with UTF-16 IME support, grapheme navigation, selection,
  clipboard shortcuts, and horizontal scrolling. Password fields disable copying.
- Blocking HTTP, encryption, password hashing, and biometric verification run on
  a dedicated worker. Vault mutations are serialized within the desktop process.

| Shortcut | Action |
| --- | --- |
| `⌘K` | Command palette (type to filter, arrows to choose, Return to run) |
| `⌘F` | Search |
| `⌘N` / `⇧⌘N` | New entry / new group |
| `⌘E` | Edit selected entry |
| `⇧⌘C` | Copy selected value |
| `⌘R` | Refresh the selected backend |
| `⌘,` | Settings |
| `⇧⌘L` | Lock and clear the desktop session |
| `↑` / `↓` | Select entries |
| `Tab` / `⇧Tab` | Move between input fields |
| `Escape` | Close a dialog, hide a value, or clear search |

## Build and run

Requires macOS 12+, current stable Rust, and Xcode Command Line Tools:

```sh
cargo run --release
cargo run --release -- --preview  # built-in masked fixtures; writes are disabled
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/bundle-macos.sh
```

The bundle script produces `dist/ZeroApiKey.app` and
`dist/ZeroApiKey_0.2.0_<architecture>.dmg`. It uses the existing `ZeroApiKey Local
Dev` signing identity when available, or an ad-hoc signature otherwise. Previous
output bundles are moved to Trash before replacement. It does not replace the
installed app in `/Applications`.

The default `runtime-shaders` feature allows builds with Command Line Tools
alone; Metal compiles and caches the shaders on the device. With full Xcode,
`PRECOMPILE_SHADERS=1 ./scripts/bundle-macos.sh` embeds precompiled shaders to
reduce first-launch work. Dependencies are pinned in `Cargo.lock`; GPUI is fixed
at 0.2.2.

Release builds use size optimization, thin LTO, one codegen unit and stripped
symbols. Measure the actual `.app`, DMG, startup and memory use for your machine;
GPUI does not inherently guarantee a smaller bundle than system-WebView Tauri.

## Architecture

```text
crates/desktop                 crates/core
  GPUI views + native input      Service (desktop unlock gate)
  indexed metadata search         │
  serial background worker ──────┼── LocalStore → encrypted vault
                                 ├── CloudStore → Infisical REST API
zak CLI ─────────────────────────┴── Store / config / importer / models
```

`crates/core` retains the original encryption and data format. The desktop
service exposes masked entry summaries and only obtains plaintext for explicit
reveal, copy, editing, import, export and sync operations. Locking clears the
view's catalog, search cache, revealed values and forms. A session generation
counter discards results from requests started before locking.

## Existing data and security

Release builds continue using `~/.zero-api-key/config.toml`, `vault.enc` and
`master.key`. The Keychain service and bundle identifier are unchanged. Debug
builds use separate `*-dev` data/config names and Keychain service.

- Vault data is JSON encrypted with XChaCha20-Poly1305.
- The random 32-byte master key is stored with `0600` permissions, following the
  filesystem permission model of an SSH private key. A legacy Keychain key is
  migrated once when needed.
- Cloud credentials are stored in macOS Keychain, not in the config file.
- The desktop password uses PBKDF2-SHA256. A password or enabled Touch ID locks
  the app at startup; every desktop data operation checks the session gate.
- Touch ID calls macOS LocalAuthentication in both debug and release builds,
  falling back to device-owner authentication when biometrics are unavailable.
- The desktop password gates app access; it does not derive the vault's master
  key. Processes running as your user can access the master key file. The CLI
  intentionally retains its original access without a GUI password prompt.
- Copied values remain on the system clipboard until replaced.
- Local self-signing is not Developer ID notarization; downloaded bundles may
  require right-click → Open.

Theme/language preferences live in `desktop.json` (`desktop-dev.json` in debug).
For an isolated local vault, set `ZERO_API_KEY_DATA_DIR` to a separate directory.
This bypasses legacy Keychain key migration; cloud credentials still use the
normal build-specific Keychain service. Tests use temporary directories and
fixture secrets, never the normal vault.

## CLI

```sh
cargo build --release -p zero-api-key-core --bin zak
./install-cli.sh  # installs and signs ~/.cargo/bin/zak with your local identity

zak init
zak add <group> [--local|--cloud]
zak get <group|KEY> [--key KEY] [--reveal] [--raw] [--local|--cloud]
zak list [group] [--local|--cloud]
zak rm <group> [--key KEY] [--local|--cloud]
zak rename <group> <OLD> <NEW> [--local|--cloud]
zak export <group> [--local|--cloud]
zak import <file.ini> [--local|--cloud]
zak import-json <file.json> [--local|--cloud]
zak sync [--to cloud|--to local] [--force]
```

`--local` / `--cloud` override the configured default backend. `zak get` resolves
its argument as a group first, then searches all groups for a key/environment
variable name. Ambiguous matches list the locations; `--raw --reveal` prints a
single value for scripts. The CLI and desktop use the same vault/config.

CLI sync prompts per conflict unless `--force` is given. Desktop sync explicitly
confirms the direction and uses source-wins replacement. Importing existing
(group, key) pairs also replaces their values after the preview confirmation.

## Cloud setup

Create an Infisical project and Machine Identity, enable Universal Auth, and grant
the identity write access to the project. Enter API base, project ID, environment,
client ID and client secret in Settings → Cloud, or run `zak init`.

## License

[MIT](LICENSE). GPUI and the adapted native input implementation are
Apache-2.0; see [third-party notices](THIRD_PARTY_NOTICES.md).
