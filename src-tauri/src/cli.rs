use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "zak", version, about = "个人密钥管理 CLI（本地加密保险库 + Infisical Cloud 双后端）")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
    /// 使用本地加密保险库后端
    #[arg(long, global = true, conflicts_with = "cloud")]
    pub local: bool,
    /// 使用 Infisical Cloud 后端
    #[arg(long, global = true)]
    pub cloud: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// 交互选择配置 local / cloud 后端并录入凭证
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
    /// 容错解析旧 ini 文件，逐条确认后写入当前后端
    Import { file: PathBuf },
    /// 在 local 与 cloud 之间同步
    Sync {
        /// 目标后端：cloud 或 local；缺省交互选择
        #[arg(long, value_parser = ["cloud", "local"])]
        to: Option<String>,
        /// 冲突时直接用源覆盖目标，不再逐条询问
        #[arg(long)]
        force: bool,
    },
}
