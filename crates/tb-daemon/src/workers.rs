//! The device run closes a pipe to wake every reader before joining it.

use std::io::{self, PipeReader, PipeWriter};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

/// The simulator retains its existing process-lifetime background readers.
#[derive(Default)]
pub(super) struct Workers {
    wake: Option<PipeWriter>,
    stop: Option<Arc<PipeReader>>,
    joined: Vec<JoinHandle<()>>,
}

impl Workers {
    pub(super) fn new() -> io::Result<Self> {
        let (stop, wake) = io::pipe()?;
        Ok(Self { wake: Some(wake), stop: Some(Arc::new(stop)), joined: Vec::new() })
    }

    pub(super) fn stop_reader(&self) -> Option<Arc<PipeReader>> {
        self.stop.clone()
    }

    pub(super) fn spawn(
        &mut self,
        name: &str,
        join: bool,
        task: impl FnOnce() + Send + 'static,
    ) -> io::Result<()> {
        let worker = thread::Builder::new().name(name.to_owned()).spawn(task)?;
        if join && self.stop.is_some() {
            self.joined.push(worker);
        }
        Ok(())
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        drop(self.wake.take());
        for worker in self.joined.drain(..) {
            if worker.join().is_err() {
                crate::log::error("a device run worker panicked");
            }
        }
    }
}
