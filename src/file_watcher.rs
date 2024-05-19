use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver};
use std::thread::{sleep, spawn};
use std::time::Duration;
use log::error;

use notify::{RecursiveMode, Watcher};

pub(crate) trait FileWatcher: Sync + Send {
    fn watch(&self, path: String) -> Result<Receiver<()>, notify::Error>;
}

pub(crate) struct NotifyFileWatcher;

impl NotifyFileWatcher {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl FileWatcher for NotifyFileWatcher {
    fn watch(&self, path: String) -> Result<Receiver<()>, notify::Error> {
        let (sender, receiver) = mpsc::channel();
        let sender = sender.clone();
        let mut watcher = notify::recommended_watcher(move |res| match res {
            Ok(_) => {
                sender.send(()).expect("Send message failed!");
            }
            Err(e) => panic!("error while watching the file system: {:}", e),
        })
        .expect("init watcher failed");
        spawn(move || {
            loop {
                match watcher
                    .watch(Path::new(&path), RecursiveMode::NonRecursive){
                    Ok(_) => {}
                    Err(err) => {
                        error!("Error watching binary file: {}", err.to_string());
                        sleep(Duration::from_secs(1));
                    }
                }
            }
        });

        Ok(receiver)
    }
}