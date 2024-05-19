use clap::Parser;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "rust-bin-reloader", about, long_about = None)]
pub struct Cli {
    #[arg(required = true, help = "Path to binary file")]
    pub(crate) path: String,

    #[arg(
    short = 'd',
    help = "Delay when restarting the binary after an unexpected exit",
    default_value = "3s",
    value_parser = parse_duration
    )]
    pub(crate) restart_delay: Duration,

    #[arg(last = true, help = "Additional arguments to pass to the binary")]
    pub(crate) binary_args: Vec<String>,
}

fn parse_duration(v: &str) -> Result<Duration, String> {
    match go_parse_duration::parse_duration(v) {
        Ok(v) => Ok(Duration::from_nanos(v as u64)),
        Err(err) => Err(format!("{:?}", err)),
    }
}
