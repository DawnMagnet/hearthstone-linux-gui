use hearthstone_linux::auth::LocalCallbackServer;
use hearthstone_linux::install::manager::TaskEvent;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(crate) struct LoginSession {
    cancel: Arc<AtomicBool>,
    callback: Rc<LocalCallbackServer>,
}

impl LoginSession {
    pub(crate) fn new(callback: Rc<LocalCallbackServer>) -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            callback,
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.callback.cancel.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

pub(crate) enum UiMessage {
    InstallEvent(TaskEvent),
    Tick,
}
