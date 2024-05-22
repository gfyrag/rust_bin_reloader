use std::process::{Command, ExitStatus};
use std::sync::mpsc::Sender;
use std::sync::{mpsc, Arc};
use std::thread::{sleep, spawn};
use std::time::Duration;

use libc::{kill, pid_t, SIGTERM};
use log::{debug, error, info};

use crate::file_watcher::{FileWatcher};

#[derive(Clone)]
pub struct ProcessConfiguration {
    pub(crate) path: String,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) restart_delay: Option<Duration>,
}

static DEFAULT_RELOAD_DELAY: Duration = Duration::from_secs(3);

pub struct ProcessExecutor {
    configuration: ProcessConfiguration,
    file_watcher: Arc<dyn FileWatcher>,
}

#[derive(Debug)]
enum Event {
    BinaryChanged,
    ProcessExited(ExitStatus),
    DelayBeforeRestartElapsed,
    Error(String),
}

impl ProcessExecutor {
    pub(crate) fn new(
        configuration: ProcessConfiguration,
        file_watcher: Arc<dyn FileWatcher>,
    ) -> Self {
        ProcessExecutor {
            configuration,
            file_watcher,
        }
    }

    pub(crate) fn start(&self) -> Option<String> {
        let configuration = self.configuration.clone();
        let (sender, receiver) = mpsc::channel();
        let mut pid: Option<pid_t> = None;
        self.run_process(sender.clone());
        self.watch_file(sender.clone());
        let mut is_waiting_process_exiting = false;

        debug!("Wait events");
        loop {
            let event = receiver.recv().unwrap();
            match event {
                Event::BinaryChanged => {
                    info!("Binary file changed");
                    match pid {
                        None => pid = self.run_process(sender.clone()),
                        Some(p) => unsafe {
                            info!("Killing process: {}", p);
                            is_waiting_process_exiting = true;
                            kill(p, SIGTERM);
                            pid = None;
                        }
                    }
                }
                Event::ProcessExited(exit_status) => {
                    if is_waiting_process_exiting {
                        info!("Process exited as expected, start a new one");
                        self.run_process(sender.clone());
                        continue
                    }
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
                        sender.send(Event::DelayBeforeRestartElapsed).expect("Send event failed!");
                    });
                }
                Event::DelayBeforeRestartElapsed => {
                    info!("Time elapsed");
                    pid = self.run_process(sender.clone());
                }
                Event::Error(err) => {
                    return Some(err);
                }
            }
        }
    }

    fn run_process(&self, sender: Sender<Event>) -> Option<pid_t> {
        let args = self.configuration.args.clone().unwrap_or_default();
        let path = self.configuration.path.clone();
        info!("Start new process");
        let child = Command::new(&path).args(args).spawn();
        match child {
            Ok(mut v) => {
                let id = v.id().clone();
                spawn(move || {
                    sender
                        .send(Event::ProcessExited(v.wait().unwrap()))
                        .expect("Send event failed!");
                });
                Some(id as pid_t)
            },
            Err(err) => {
                error!("unable to start process: {}", err);
                None
            },
        }
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
