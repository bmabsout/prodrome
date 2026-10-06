//! The binary: parse, run one verb, print what it said, and on the standard
//! error what its reading left out.
//!
//! Three exit codes and no fourth: 0 for an answer, 1 for a verdict that
//! failed (`verify` with findings), 2 for a refusal.

#![forbid(unsafe_code)]

use clap::Parser;

use prodrome_cli::command::Cli;
use prodrome_cli::run;

fn main() -> std::process::ExitCode {
    match run(&Cli::parse()) {
        Ok(outcome) => {
            for line in &outcome.left_out {
                eprintln!("prodrome: {line}");
            }
            if !outcome.text.is_empty() {
                println!("{}", outcome.text);
            }
            if outcome.ok {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("prodrome: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
