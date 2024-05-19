#![deny(unused_crate_dependencies)]

use std::process;
use std::process::exit;
use std::sync::Arc;
use std::thread::spawn;

use clap::Parser;
use libc::{SIGINT, SIGQUIT, SIGTERM};
use log::{debug, error, info, LevelFilter};
use signal_hook::iterator::Signals;
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

    let mut signals = Signals::new(&[SIGINT, SIGTERM, SIGQUIT])
        .expect("Configuring signals");

    spawn(move || {
        for sig in signals.forever() {
            debug!("Received signal {:?}", sig);
            exit(1);
        }
    });

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
