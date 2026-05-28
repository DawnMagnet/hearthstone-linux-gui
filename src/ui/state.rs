use hearthstone_linux::install::manager::TaskEvent;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(crate) struct LoginSession {
    cancel: Arc<AtomicBool>,
}

impl LoginSession {
    pub(crate) fn new() -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

pub(crate) enum UiMessage {
    InstallEvent(TaskEvent),
    Tick,
}
