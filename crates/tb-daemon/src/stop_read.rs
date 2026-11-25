use std::io::{self, PipeReader, Read};
use std::os::fd::AsFd;
use std::sync::Arc;

use rustix::event::{PollFd, PollFlags, poll};

/// Exclusive reader of a pollable descriptor. EOF on the stop pipe takes precedence over
/// available input, so a busy reader can still be cancelled.
pub struct StopRead<F>(pub F, pub Option<Arc<PipeReader>>);

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
