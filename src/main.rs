use process::{Command, ExitStatus};
use std::path::Path;
use std::process;
use std::sync::{Arc, mpsc};
use std::sync::mpsc::{Receiver, RecvError, Sender};
use std::thread::{sleep, spawn};
use std::time::Duration;

use clap::{Arg, arg, Command as ClapCommand};
use go_parse_duration::parse_duration;
use libc::{kill, pid_t, SIGTERM};
use log::{info, LevelFilter};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use simplelog::{ColorChoice, CombinedLogger, Config, TerminalMode, TermLogger};

fn exit<V>(status_code: i32, msg: String) -> V {
    print!("{}", msg);
    process::exit(status_code);
}

fn main() {
    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
        ]
    ).unwrap();

    info!("Starting...");
    let matches = ClapCommand::new("rpod")
        .bin_name("rpod")
        .args([
            arg!(restart_delay: -d <DELAY>)
                .help("Delay when restarting the binary after an unexpected exit")
                .default_value("3s"),
            Arg::new("path")
                .help("Path to binary file")
                .required(true),
            arg!(binary_args: [BINARY_ARGS])
                .num_args(1..)
                .last(true),
        ])
        .get_matches();

    let binary_path = match matches.get_one::<String>("path") {
        None => panic!("Should not happen, arg 'path' is required"),
        Some(v) => v,
    }.clone();
    info!("Detect binary: {:?}", binary_path);

    let restart_delay = match matches.get_one::<String>("restart_delay") {
        None => Duration::from_secs(3),
        Some(v) => match parse_duration(v) {
            Ok(v) => Duration::from_nanos(v as u64),
            Err(err) => exit(1, format!("{:?}", err)),
        },
    };
    info!("Restart delay: {:}s", restart_delay.as_secs());

    let binary_args = match matches.get_many::<String>("binary_args") {
        None => vec![],
        Some(v) => v.map(|v| v.clone()).collect(),
    };
    info!("Detect binary args: {:?}", binary_args);

    ProcessExecutor::new(
        ProcessConfiguration{
            path: binary_path,
            args: Some(binary_args),
            restart_delay: Some(restart_delay),
        },
        Arc::new(NotifyFileWatcher::new())
    ).start();

    info!("Wait signal...");
    loop {}
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

trait FileWatcher: Sync + Send {
    type WatcherType;
    fn watch(&self, path: String) -> Result<Self::WatcherType, notify::Error>;
}

struct NotifyFileWatcher;

impl NotifyFileWatcher {
    fn new() -> Self {
        Self{}
    }
}

impl FileWatcher for NotifyFileWatcher {
    type WatcherType = DefaultWatch;

    fn watch(&self, path: String) -> Result<DefaultWatch, notify::Error> {
        let (sender, receiver) = mpsc::channel();
        let sender = sender.clone();
        let mut watcher = notify::recommended_watcher(
            move |res| {
                match res {
                    Ok(_) => {
                        sender
                            .send(())
                            .map(|v| ())
                            .expect("Send message failed!");
                    },
                    Err(e) => panic!("error while watching the file system: {:}", e),
                }
            })?;
        let _guard = watcher.watch(Path::new(&path), RecursiveMode::NonRecursive)?;
        Ok(DefaultWatch::new(watcher, receiver))
    }
}

trait Watch {
    fn recv(&self) -> Result<(), RecvError>;
}

struct DefaultWatch {
    watcher: RecommendedWatcher,
    receiver: Receiver<()>
}

impl Watch for DefaultWatch {
    fn recv(&self) -> Result<(), RecvError> {
        self.receiver.recv()
    }
}

impl DefaultWatch {
    fn new(watcher: RecommendedWatcher, receiver: Receiver<()>) -> Self {
        Self{ watcher, receiver }
    }
}