use std::process::{Command, ExitStatus};
use std::sync::{Arc, mpsc};
use std::sync::mpsc::Sender;
use std::thread::{sleep, spawn};
use std::time::Duration;

use libc::{kill, pid_t, SIGTERM};
use log::{debug, info};

use crate::file_watcher::{FileWatcher, Watch};

#[derive(Clone)]
pub struct ProcessConfiguration {
    pub(crate) path: String,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) restart_delay: Option<Duration>,
}

static DEFAULT_RELOAD_DELAY: Duration = Duration::from_secs(3);

pub struct ProcessExecutor<T> {
    configuration: ProcessConfiguration,
    file_watcher: Arc<dyn FileWatcher<WatcherType=T>>,
}

#[derive(Debug)]
enum Event {
    BinaryChanged,
    ProcessExited(ExitStatus),
    TimeElapse,
    Error(String),
}

impl<T: Watch + 'static> ProcessExecutor<T> {
    pub(crate) fn new(
        configuration: ProcessConfiguration,
        file_watcher: Arc<dyn FileWatcher<WatcherType=T>>,
    ) -> Self {
        ProcessExecutor {
            configuration,
            file_watcher,
        }
    }

    pub(crate) fn start(&self) -> Option<String> {
        let configuration = self.configuration.clone();
        let (sender, receiver) = mpsc::channel();
        let mut pid = self.run_process(sender.clone());
        self.watch_file(sender.clone());

        debug!("Wait events");
        loop {
            let event = receiver.recv().unwrap();
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
                                debug!("Binary file changed again");
                                // Just waiting again
                            }
                            Event::ProcessExited(_) => {
                                info!("Process finally exited, start a new one...");
                                pid = self.run_process(sender.clone());
                                break;
                            }
                            Event::TimeElapse => {
                                info!("Time elapsed");
                                // Nothing to do, we still need to wait for the end of the process
                            }
                            Event::Error(err) => {
                                return Some(err);
                            }
                        }
                    }
                }
                Event::ProcessExited(exit_status) => {
                    let duration = configuration
                        .restart_delay
                        .unwrap_or_else(|| DEFAULT_RELOAD_DELAY);
                    info!(
                        "Process unexpectedly terminated with status: {:?}, will wait for {}s",
                        exit_status.code().unwrap(),
                        duration.as_secs()
                    );
                    let sender = sender.clone();
                    spawn(move || {
                        debug!("Wait {:?}", duration);
                        sleep(duration);
                        sender.send(Event::TimeElapse).expect("Send event failed!");
                    });
                }
                Event::TimeElapse => {
                    info!("Time elapsed");
                    pid = self.run_process(sender.clone());
                }
                Event::Error(err) => {
                    return Some(err);
                }
            }
        }
    }

    fn run_process(&self, sender: Sender<Event>) -> pid_t {
        let args = self.configuration.args.clone().unwrap_or_default();
        let path = self.configuration.path.clone();
        let mut child = Command::new(&path).args(args).spawn().unwrap();
        let ret = child.id();
        info!("Start new process");
        spawn(move || {
            sender
                .send(Event::ProcessExited(child.wait().unwrap()))
                .expect("Send event failed!");
        });
        ret as pid_t
    }

    fn watch_file(&self, sender: Sender<Event>) {
        info!("Watch binary file");
        let configuration = self.configuration.clone();
        let file_watcher = self.file_watcher.clone();
        spawn(move || {
            let file_watcher_receiver = file_watcher.watch(configuration.path.clone());

            match file_watcher_receiver {
                Ok(v) => loop {
                    match v.recv() {
                        Ok(_) => {}
                        Err(err) => panic!("{}", err.to_string()),
                    }
                    info!("Detect binary changed");
                    sender
                        .send(Event::BinaryChanged)
                        .expect("Send event failed!");
                },
                Err(err) => {
                    sender
                        .send(Event::Error(format!(
                            "error while sending file change event: {:}",
                            err
                        )))
                        .expect("Send event failed!");
                }
            }
        });
    }
}
