mod api;
mod auth;
mod commands;
mod config;
mod importer;
mod models;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "zak", version, about = "个人密钥管理 CLI（Infisical Cloud 后端）")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 初始化配置并录入 Machine Identity 凭证（入钥匙串）
    Init,
    /// 交互式向分组添加条目（api-key / login / note）
    Add { group: String },
    /// 查看分组条目，默认掩码显示
    Get {
        group: String,
        /// 只看指定键
        #[arg(long)]
        key: Option<String>,
        /// 明文显示
        #[arg(long)]
        reveal: bool,
    },
    /// 列出全部分组，或指定分组内的条目
    List { group: Option<String> },
    /// 删除整个分组或单个键（删除前确认）
    Rm {
        group: String,
        /// 只删除指定键
        #[arg(long)]
        key: Option<String>,
    },
    /// 输出 export ENV=... 行，可 eval
    Export { group: String },
    /// 容错解析旧 ini 文件，逐条确认后上传
    Import { file: PathBuf },
}

fn main() -> anyhow::Result<()> {
    commands::run(Cli::parse().command)
}
