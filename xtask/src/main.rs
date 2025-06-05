use std::process;

use anyhow::Result;
use clap::{ArgMatches, Command};

fn main() -> Result<()> {
    let args = clap::command!()
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("install")
                .about("Install binary crates")
                .arg(
                    clap::Arg::new("crate")
                        .help("Specific crate to install (installs all if not specified)")
                        .value_parser([
                            "mcp-projectfiles-bin",
                            "yolo-projectfiles",
                            "yolo-homefiles",
                            "yolo-executioner",
                            "yolo-terminator",
                            "yolo-memento",
                        ])
                        .required(false),
                ),
        )
        .get_matches();

    match args.subcommand() {
        Some(("install", args)) => handle_install_command(args),
        Some((command, _)) => anyhow::bail!("Unexpected command: {command}"),
        None => anyhow::bail!("Expected subcommand"),
    }
}

fn handle_install_command(args: &ArgMatches) -> Result<()> {
    // List of all binary crates
    let all_crates = [
        "mcp-projectfiles-bin",
        "yolo-projectfiles",
        "yolo-homefiles",
        "yolo-executioner",
        "yolo-terminator",
        "yolo-memento",
    ];

    // Determine which crates to install
    let crates_to_install: Vec<&str> = if let Some(specific_crate) = args.get_one::<String>("crate") {
        vec![specific_crate.as_str()]
    } else {
        all_crates.to_vec()
    };

    println!("Installing {} binary crate(s)...", crates_to_install.len());

    for crate_name in &crates_to_install {
        println!("\nInstalling {}...", crate_name);
        let status = process::Command::new("cargo")
            .args(["install", "--path", &format!("crates/{}", crate_name)])
            .status()?;

        if !status.success() {
            anyhow::bail!("Failed to install {}", crate_name);
        }
    }

    if crates_to_install.len() == 1 {
        println!("\n{} installed successfully!", crates_to_install[0]);
    } else {
        println!("\nAll {} crates installed successfully!", crates_to_install.len());
    }
    Ok(())
}