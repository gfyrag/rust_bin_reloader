use process::{Command, ExitStatus};
use std::path::Path;
use std::process;
use std::sync::{Arc, mpsc};
use std::sync::mpsc::{Receiver, RecvError, Sender};
use std::thread::{sleep, spawn};
use std::time::Duration;

use clap::{arg, Arg, Command as ClapCommand, Parser};
use libc::{kill, pid_t, SIGTERM};
use log::{error, info, LevelFilter};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use simplelog::{ColorChoice, CombinedLogger, Config, TerminalMode, TermLogger};
use cli::Cli;

use file_watcher::{FileWatcher, NotifyFileWatcher, Watch};
use process_executor::{ProcessConfiguration, ProcessExecutor};

mod file_watcher;
mod process_executor;
mod cli;

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
