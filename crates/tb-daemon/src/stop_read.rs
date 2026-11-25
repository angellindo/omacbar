use std::fs::File;
use std::io::{self, IsTerminal, PipeReader, Read};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::sync::Arc;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::fs::{self, FileType, Mode, OFlags};
use rustix::io::Errno;
use rustix::net::{RecvFlags, recv};
use rustix::pipe::{PipeFlags, SpliceFlags, pipe_with, splice};

/// Exclusive reader of a descriptor opened nonblocking when a stop pipe is supplied. EOF on
/// the stop pipe takes precedence over available input, so a busy reader can still be cancelled.
pub struct StopRead<F>(pub F, pub Option<Arc<PipeReader>>);

impl<F: AsFd> StopRead<F> {
    /// Inherited input may be shared with another reader. Use a nonblocking operation that leaves
    /// the inherited open file description alone, even when poll readiness is lost.
    pub fn inherited(input: F, stop: Option<Arc<PipeReader>>) -> io::Result<impl Read> {
        let mode = if stop.is_some() { InheritedMode::for_fd(&input)? } else { InheritedMode::Read };
        Ok(InheritedInput { input: Self(input, stop), mode })
    }

    fn read_with(
        &self,
        buf: &mut [u8],
        mut read: impl FnMut(&F, &mut [u8]) -> rustix::io::Result<usize>,
    ) -> io::Result<usize> {
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
                    Err(Errno::INTR) => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            match read(&self.0, buf) {
                Err(Errno::AGAIN | Errno::INTR) if self.1.is_some() => {}
                result => return result.map_err(io::Error::from),
            }
        }
    }
}

impl<F: AsFd> Read for StopRead<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read_with(buf, |fd, buf| rustix::io::read(fd, buf))
    }
}

struct InheritedInput<F> {
    input: StopRead<F>,
    mode: InheritedMode,
}

enum InheritedMode {
    Read,
    Socket,
    Pipe { read_end: OwnedFd, write_end: OwnedFd },
    Reopened(OwnedFd),
}

impl InheritedMode {
    fn for_fd(input: &impl AsFd) -> io::Result<Self> {
        let fd = input.as_fd();
        let flags = fs::fcntl_getfl(fd)?;
        if flags.intersects(OFlags::WRONLY | OFlags::PATH) {
            return Err(Errno::BADF.into());
        }
        let stat = fs::fstat(fd)?;
        match FileType::from_raw_mode(stat.st_mode) {
            // File reads have no transient readiness and must retain the inherited offset.
            FileType::RegularFile | FileType::BlockDevice => Ok(Self::Read),
            FileType::Socket => Ok(Self::Socket),
            FileType::Fifo => {
                // Both ends of splice are pipes. SPLICE_F_NONBLOCK covers the source even
                // when its inherited description is blocking or cannot be reopened.
                let (read_end, write_end) = pipe_with(PipeFlags::NONBLOCK | PipeFlags::CLOEXEC)?;
                Ok(Self::Pipe { read_end, write_end })
            }
            _ if flags.contains(OFlags::NONBLOCK) => Ok(Self::Read),
            FileType::CharacterDevice if can_reopen(fd, stat.st_rdev)? => {
                let path = format!("/proc/self/fd/{}", fd.as_raw_fd());
                let flags = OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC;
                Ok(Self::Reopened(fs::open(path.as_str(), flags, Mode::empty())?))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "cannot read this stdin descriptor without blocking; use a nonblocking descriptor",
            )),
        }
    }
}

fn can_reopen(fd: BorrowedFd<'_>, device: rustix::fs::Dev) -> io::Result<bool> {
    match (fs::major(device), fs::minor(device)) {
        (1, 3 | 5 | 7) => Ok(true), // /dev/null, /dev/zero, /dev/full have no per-open input.
        (5, 2) | (2, _) => Ok(false), // Reopening a PTY master can allocate a different terminal.
        _ => Ok(File::from(fd.try_clone_to_owned()?).is_terminal()),
    }
}

impl<F: AsFd> Read for InheritedInput<F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read_with(buf, |fd, buf| match &self.mode {
            InheritedMode::Read => rustix::io::read(fd, buf),
            InheritedMode::Socket => recv(fd, buf, RecvFlags::DONTWAIT).map(|(_, len)| len),
            InheritedMode::Pipe { read_end, write_end } => {
                let len = splice(fd, None, write_end, None, buf.len(), SpliceFlags::NONBLOCK)?;
                rustix::io::read(read_end, &mut buf[..len])
            }
            InheritedMode::Reopened(fd) => rustix::io::read(fd, buf),
        })
    }
}
