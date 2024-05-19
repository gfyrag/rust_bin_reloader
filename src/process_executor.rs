use std::time::Duration;
use std::sync::{Arc, mpsc};
use std::process::{Command, ExitStatus};
use log::info;
use libc::{kill, pid_t, SIGTERM};
use std::thread::{sleep, spawn};
use std::sync::mpsc::Sender;
use crate::file_watcher::{FileWatcher, Watch};

#[derive(Clone)]
pub struct ProcessConfiguration{
    pub(crate) path: String,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) restart_delay: Option<Duration>
}


static DEFAULT_RELOAD_DELAY: Duration = Duration::from_secs(3);

pub struct ProcessExecutor<T> {
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

    pub(crate) fn new(configuration: ProcessConfiguration, file_watcher: Arc<dyn FileWatcher<WatcherType=T>>) -> Self {
        ProcessExecutor{ configuration, file_watcher }
    }

    pub(crate) fn start(&self) -> Option<String> {
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