use process::{Command, ExitStatus};
use std::path::Path;
use std::process;
use std::sync::{Arc, mpsc};
use std::sync::mpsc::{Receiver, RecvError, Sender};
use std::thread::{sleep, spawn};
use std::time::Duration;

use clap::{Arg, arg, Command as ClapCommand, Parser};
use go_parse_duration::parse_duration as go_parse_duration;
use libc::{kill, pid_t, SIGTERM};
use log::{error, info, LevelFilter};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use simplelog::{ColorChoice, CombinedLogger, Config, TerminalMode, TermLogger};
use file_watcher::{FileWatcher, NotifyFileWatcher, Watch};
use process_executor::{ProcessConfiguration, ProcessExecutor};

mod file_watcher;
mod process_executor;

#[derive(Parser, Debug)]
#[command(name = "rpod", about, long_about = None)]
struct Cli {
    #[arg(required = true, help = "Path to binary file")]
    path: String,

    #[arg(
        short = 'd',
        help = "Delay when restarting the binary after an unexpected exit",
        default_value = "3s",
        value_parser = parse_duration
    )]
    restart_delay: Duration,

    #[arg(last = true, help = "Additional arguments to pass to the binary")]
    binary_args: Vec<String>
}

fn parse_duration(v: &str) -> Result<Duration, String> {
    match go_parse_duration(v) {
        Ok(v) => Ok(Duration::from_nanos(v as u64)),
        Err(err) => Err(format!("{:?}", err)),
    }
}

fn init_logger() {
    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        ]
    ).unwrap();
}

fn main() {
    let args = Cli::parse();
    init_logger();

    info!("Starting...");
    let option = ProcessExecutor::new(
        ProcessConfiguration {
            path: args.path,
            args: Some(args.binary_args),
            restart_delay: Some(args.restart_delay),
        },
        Arc::new(NotifyFileWatcher::new())
    ).start();
    if option.is_some() {
        error!("{}", option.unwrap());
        process::exit(1);
    }
}
