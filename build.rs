use crate::cli::Cli;

#[path="src/cli.rs"]
mod cli;

fn main() -> std::io::Result<()> {
    let md = clap_markdown::help_markdown::<Cli>();
    std::fs::write("./README.md", md)?;

    Ok(())
}