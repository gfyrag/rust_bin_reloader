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

mod file_watcher;

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

#[derive(Clone)]
struct ProcessConfiguration{
    path: String,
    args: Option<Vec<String>>,
    restart_delay: Option<Duration>
}


static DEFAULT_RELOAD_DELAY: Duration = Duration::from_secs(3);
struct ProcessExecutor<T> {
    configuration: ProcessConfiguration,
    file_watcher: Arc<dyn FileWatcher<WatcherType=T>>
}

#[derive(Debug)]
enum Event {
    BinaryChanged,
    ProcessExited(ExitStatus),
    TimeElapse,
    Error(String)
}

impl<T: Watch + 'static> ProcessExecutor<T> {

    fn new(configuration: ProcessConfiguration, file_watcher: Arc<dyn FileWatcher<WatcherType=T>>) -> Self {
        ProcessExecutor{ configuration, file_watcher }
    }

    fn start(&self) -> Option<String> {
        let configuration = self.configuration.clone();
        let (sender, receiver) = mpsc::channel();
        let mut pid = self.run_process(sender.clone());
        self.watch_file(sender.clone());

        info!("Wait events");
        loop {
            let event = receiver.recv().unwrap();
            info!("Got event");
            match event {
                Event::BinaryChanged => {
                    info!("Binary file changed, kill sub command");
                    unsafe {
                        kill(pid, SIGTERM);
                    }
                    loop {
                        let event = receiver.recv().unwrap();
                        match event {
                            Event::BinaryChanged => {
                                info!("Binary file changed again");
                                // Just waiting again
                            },
                            Event::ProcessExited(_) => {
                                info!("Process finally exited, start a new one...");
                                pid = self.run_process(sender.clone());
                                break
                            },
                            Event::TimeElapse => {
                                info!("Time elapsed");
                                // Nothing to do, we still need to wait for the end of the process
                            },
                            Event::Error(err) => {
                                return Some(err);
                            },
                        }
                    }
                    //
                }
                Event::ProcessExited(exit_status) => {
                    info!("Process unexpectedly terminated with status: {:?}", exit_status);
                    let sender = sender.clone();
                    spawn(move || {
                        let duration = configuration.restart_delay.unwrap_or_else(|| DEFAULT_RELOAD_DELAY);
                        info!("Wait {:?}", duration);
                        sleep(duration);
                        sender
                            .send(Event::TimeElapse)
                            .expect("Send event failed!");
                    });
                },
                Event::TimeElapse => {
                    info!("Time elapsed");
                    pid = self.run_process(sender.clone());
                }
                Event::Error(err) => {
                    return Some(err);
                },
            }
        };
    }

    fn run_process(&self, sender: Sender<Event>) -> pid_t {
        let args = self.configuration.args.clone().unwrap_or_default();
        let path = self.configuration.path.clone();
        let mut child = Command::new(&path)
            .args(args)
            .spawn()
            .unwrap();
        let ret = child.id();
        spawn(move || {
            info!("Start process");
            let exit_status = child.wait().unwrap();

            info!("process ended");
            sender
                .send(Event::ProcessExited(exit_status))
                .expect("Send event failed!");
        });
        ret as pid_t
    }

    fn watch_file(&self, sender: Sender<Event>) {
        info!("Watch binary file");
        let configuration = self.configuration.clone();
        let file_watcher = self.file_watcher.clone();
        spawn(move || {
            let file_watcher_receiver = file_watcher
                .watch(configuration.path.clone());

            match file_watcher_receiver {
                Ok(mut v) => loop {
                    match v.recv() {
                        Ok(_) => {}
                        Err(err) => panic!("{}", err.to_string())
                    }
                    info!("Detect binary changed");
                    sender
                        .send(Event::BinaryChanged)
                        .expect("Send event failed!");
                },
                Err(err) => {
                    sender
                        .send(Event::Error(format!("error while sending file change event: {:}", err)))
                        .expect("Send event failed!");
                },
            }
        });
    }
}
