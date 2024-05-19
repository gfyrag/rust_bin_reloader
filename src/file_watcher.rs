use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::mpsc::{Receiver, RecvError};
use std::sync::mpsc;
use std::path::Path;

pub(crate) trait FileWatcher: Sync + Send {
    type WatcherType;
    fn watch(&self, path: String) -> Result<Self::WatcherType, notify::Error>;
}

pub(crate) struct NotifyFileWatcher;

impl NotifyFileWatcher {
    pub(crate) fn new() -> Self {
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

pub(crate) trait Watch {
    fn recv(&self) -> Result<(), RecvError>;
}

pub(crate) struct DefaultWatch {
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
