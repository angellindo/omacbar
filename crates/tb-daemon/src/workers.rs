//! The device run closes a pipe to wake every reader before joining it.

use std::io::{self, PipeReader, PipeWriter, Read};
use std::os::fd::AsFd;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use rustix::event::{PollFd, PollFlags, poll};

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

/// Exclusive reader of a pollable descriptor. EOF on the stop pipe takes precedence over
/// available input, so a busy reader can still be cancelled.
pub(super) struct StopRead<F>(pub F, pub Option<Arc<PipeReader>>);

impl<F: AsFd> Read for StopRead<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if let Some(stop) = &self.1 {
                let mut fds =
                    [PollFd::new(stop.as_ref(), PollFlags::IN), PollFd::new(&self.0, PollFlags::IN)];
                match poll(&mut fds, None) {
                    Ok(_) if !fds[0].revents().is_empty() => return Ok(0),
                    Ok(_) => {}
                    Err(rustix::io::Errno::INTR) => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            match rustix::io::read(&self.0, &mut *buf) {
                Err(rustix::io::Errno::AGAIN) if self.1.is_some() => {}
                result => return result.map_err(io::Error::from),
            }
        }
    }
}
