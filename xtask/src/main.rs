use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use clap::Parser;
use console::style;
use duct::cmd;

#[derive(clap::Parser, Debug)]
struct CopyTestAction {
    #[arg(long)]
    week: usize,
    #[arg(long)]
    day: usize,
}

#[derive(clap::Subcommand, Debug)]
enum Action {
    /// Check.
    Check,
    /// Build and serve book.
    Book,
    /// Install necessary tools for development.
    InstallTools,
    /// Show environment variables.
    Show,
    /// Run CI jobs
    Ci,
    /// Sync starter repo and reference solution.
    Sync,
    /// Check starter code
    Scheck,
    /// Copy test cases
    CopyTest(CopyTestAction),
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

fn switch_to_starter_root() -> Result<()> {
    std::env::set_current_dir(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| anyhow!("failed to find the workspace root"))?
            .join("mini-lsm-starter"),
    )?;
    Ok(())
}

fn fmt() -> Result<()> {
    println!("{}", style("cargo fmt").bold());
    cmd!("cargo", "fmt").run()?;
    Ok(())
}

fn check_fmt() -> Result<()> {
    println!("{}", style("cargo fmt --check").bold());
    cmd!("cargo", "fmt", "--check").run()?;
    Ok(())
}

fn check() -> Result<()> {
    println!("{}", style("cargo check").bold());
    cmd!("cargo", "check", "--all-targets").run()?;
    Ok(())
}

fn test() -> Result<()> {
    println!("{}", style("cargo nextest run").bold());
    cmd!("cargo", "nextest", "run").run()?;
    Ok(())
}

fn clippy() -> Result<()> {
    println!("{}", style("cargo clippy").bold());
    cmd!("cargo", "clippy", "--all-targets").run()?;
    Ok(())
}

fn build_book() -> Result<()> {
    println!("{}", style("mdbook build").bold());
    cmd!("mdbook", "build").dir("mini-lsm-book").run()?;
    Ok(())
}

fn serve_book() -> Result<()> {
    println!("{}", style("mdbook serve").bold());
    cmd!("mdbook", "serve").dir("mini-lsm-book").run()?;
    Ok(())
}

fn sync() -> Result<()> {
    cmd!("mkdir", "-p", "sync-tmp").run()?;
    cmd!("cp", "-a", "mini-lsm-starter/", "sync-tmp/mini-lsm-starter").run()?;
    let cargo_toml = "sync-tmp/mini-lsm-starter/Cargo.toml";
    std::fs::write(
        cargo_toml,
        std::fs::read_to_string(cargo_toml)?.replace("mini-lsm-starter", "mini-lsm")
            + "\n[workspace]\n",
    )?;
    let wrapper_rs = "sync-tmp/mini-lsm-starter/src/bin/wrapper.rs";
    std::fs::write(
        wrapper_rs,
        std::fs::read_to_string(wrapper_rs)?.replace("mini_lsm_starter", "mini_lsm"),
    )?;
    cmd!(
        "cargo",
        "semver-checks",
        "check-release",
        "--manifest-path",
        cargo_toml,
        "--baseline-root",
        "mini-lsm/Cargo.toml",
    )
    .run()?;
    Ok(())
}

fn copy_test_case(test: CopyTestAction) -> Result<()> {
    use std::fmt::Write;
    let src_dir = if test.week >= 3 {
        "mini-lsm-mvcc/src/tests"
    } else {
        "mini-lsm/src/tests"
    };
    let target_dir = "mini-lsm-starter/src/tests";
    if !Path::new(target_dir).exists() {
        std::fs::create_dir(target_dir)?;
    }
    let test_filename = format!("week{}_day{}.rs", test.week, test.day);
    let src = format!("{}/{}", src_dir, test_filename);
    let target = format!("{}/{}", target_dir, test_filename);
    cmd!("cp", src, target).run()?;
    let test_filename = "harness.rs";
    let src = format!("{}/{}", src_dir, test_filename);
    let target = format!("{}/{}", target_dir, test_filename);
    cmd!("cp", src, target).run()?;
    let mut test_file = Vec::new();
    for file in Path::new(&target_dir).read_dir()? {
        let file = file?;
        let fname = file.file_name();
        let fnamestr = fname
            .as_os_str()
            .to_str()
            .ok_or_else(|| anyhow!("invalid filename?"))?;
        if let Some((mod_name, _)) = fnamestr.split_once(".rs") {
            test_file.push(mod_name.to_string());
        }
    }
    let mut tests_mod = String::new();
    writeln!(tests_mod, "//! DO NOT MODIFY -- Mini-LSM tests modules")?;
    writeln!(
        tests_mod,
        "//! This file will be automatically rewritten by the copy-test command."
    )?;
    writeln!(tests_mod)?;
    for tf in test_file {
        writeln!(tests_mod, "mod {};", tf)?;
    }
    println!("{}", tests_mod);
    std::fs::write("mini-lsm-starter/src/tests.rs", tests_mod)?;
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
            println!("{}", style("cargo install cargo-semver-checks").bold());
            cmd!("cargo", "install", "cargo-semver-checks", "--locked").run()?;
        }
        Action::Check => {
            switch_to_workspace_root()?;
            fmt()?;
            check()?;
            test()?;
            clippy()?;
        }
        Action::Scheck => {
            switch_to_starter_root()?;
            fmt()?;
            check()?;
            test()?;
            clippy()?;
        }
        Action::Book => {
            switch_to_workspace_root()?;
            serve_book()?;
        }
        Action::Ci => {
            switch_to_workspace_root()?;
            check_fmt()?;
            check()?;
            test()?;
            clippy()?;
            build_book()?;
        }
        Action::Show => {
            println!("CARGO_MANIFEST_DIR={}", env!("CARGO_MANIFEST_DIR"));
            println!("PWD={:?}", std::env::current_dir()?);
        }
        Action::Sync => {
            switch_to_workspace_root()?;
            sync()?;
        }
        Action::CopyTest(test) => {
            switch_to_workspace_root()?;
            copy_test_case(test)?;
        }
    }

    Ok(())
}