#![deny(unused_crate_dependencies)]

use std::process;
use std::sync::Arc;

use clap::Parser;
use log::{error, info, LevelFilter};
use simplelog::{ColorChoice, CombinedLogger, Config, TerminalMode, TermLogger};

use cli::Cli;
use file_watcher::NotifyFileWatcher;
use process_executor::{ProcessConfiguration, ProcessExecutor};

mod cli;
mod file_watcher;
mod process_executor;

fn init_logger() {
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
        .unwrap();
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
        Arc::new(NotifyFileWatcher::new()),
    )
        .start();
    if option.is_some() {
        error!("{}", option.unwrap());
        process::exit(1);
    }
}
