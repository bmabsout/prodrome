//! The binary: parse, apply one delivery, print what it said.
//!
//! Exit 0 for an answer and 2 for a refusal, as `prodrome` does.

#![forbid(unsafe_code)]

use clap::Parser;

use prodrome_github::{run, Cli};

fn main() -> std::process::ExitCode {
    match run(&Cli::parse()) {
        Ok(outcome) => {
            if !outcome.text.is_empty() {
                println!("{}", outcome.text);
            }
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("prodrome-github: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
