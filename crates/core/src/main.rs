use clap::Parser;
use zero_api_key_lib::cli::Cli;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    zero_api_key_lib::commands::run(cli.command, cli.local, cli.cloud)
}
