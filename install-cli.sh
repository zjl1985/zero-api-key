#!/usr/bin/env bash
# 安装 zak CLI 并用自签名证书签名——签名身份稳定，钥匙串「始终允许」才能跨重装生效
set -euo pipefail
cd "$(dirname "$0")/src-tauri"
cargo install --path . --bin zak
codesign --force --sign "ZeroApiKey Local Dev" ~/.cargo/bin/zak
echo "zak 已安装并签名: $(which zak)"
