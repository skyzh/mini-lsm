use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Parser;
use console::style;
use duct::cmd;

#[derive(clap::Subcommand, Debug)]
enum Action {
    /// Check.
    Check,
    /// Install necessary tools for development.
    InstallTools,
    /// Show environment variables.
    Show,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    action: Action,
}

fn switch_to_workspace_root() -> Result<()> {
    std::env::set_current_dir(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| anyhow!("failed to find the workspace root"))?,
    )?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.action {
        Action::InstallTools => {
            println!("{}", style("cargo install cargo-nextest").bold());
            cmd!("cargo", "install", "cargo-nextest", "--locked").run()?;
            println!("{}", style("cargo install mdbook mdbook-toc").bold());
            cmd!("cargo", "install", "mdbook", "mdbook-toc", "--locked").run()?;
        }
        Action::Check => {
            switch_to_workspace_root()?;
            println!("{}", style("cargo fmt").bold());
            cmd!("cargo", "fmt").run()?;
            println!("{}", style("cargo check").bold());
            cmd!("cargo", "check", "--all-targets").run()?;
            println!("{}", style("cargo nextest run").bold());
            cmd!("cargo", "nextest", "run").run()?;
            println!("{}", style("cargo clippy").bold());
            cmd!("cargo", "clippy", "--all-targets").run()?;
            println!("{}", style("mdbook build").bold());
            cmd!("mdbook", "build").dir("mini-lsm-book").run()?;
        }
        Action::Show => {
            println!("CARGO_MANIFEST_DIR={}", env!("CARGO_MANIFEST_DIR"));
            println!("PWD={:?}", std::env::current_dir()?);
        }
    }

    Ok(())
}
