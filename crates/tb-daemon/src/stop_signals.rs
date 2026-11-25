//! Thread-local stop signal masking and the synchronous waiter (ADR-0014).
#![allow(unsafe_code, reason = "POSIX signal masks and sigwait")]

use std::io;
use std::mem::MaybeUninit;
use std::os::unix::{process::CommandExt, thread::JoinHandleExt};
use std::process::Command;
use std::sync::mpsc;
use std::thread;

/// The stop set and the calling thread's mask before it was blocked.
pub struct StopSignals {
    set: libc::sigset_t,
    previous: libc::sigset_t,
}

/// Blocks HUP, INT and TERM on the calling thread. Call before creating other threads, which
/// inherit the mask. The daemon keeps it blocked until process exit.
pub fn block_stop_signals() -> io::Result<StopSignals> {
    // Linux may write only the kernel-sized part of the old mask.
    let mut set = MaybeUninit::zeroed();
    let mut previous = MaybeUninit::zeroed();
    // SAFETY: both pointers name zero-initialized sigset_t storage. The calls fill a
    // valid set and save the previous mask on success.
    unsafe {
        if libc::sigemptyset(set.as_mut_ptr()) != 0 {
            return Err(io::Error::last_os_error());
        }
        for signal in [libc::SIGHUP, libc::SIGINT, libc::SIGTERM] {
            if libc::sigaddset(set.as_mut_ptr(), signal) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        check(libc::pthread_sigmask(libc::SIG_BLOCK, set.as_ptr(), previous.as_mut_ptr()))?;
        Ok(StopSignals { set: set.assume_init(), previous: previous.assume_init() })
    }
}

impl StopSignals {
    pub(crate) fn command_mask(&self) -> CommandSignalMask {
        CommandSignalMask(self.previous)
    }

    /// Block the stop set on this thread and wait for it on a dedicated thread.
    /// Keep the thread alive until the guard is dropped
    /// so its thread ID stays valid even if a real stop arrives during shutdown.
    pub fn watch(self, report: impl FnOnce(io::Result<()>) + Send + 'static) -> io::Result<StopWaiter> {
        // SAFETY: set is initialized. Reassert the mask before spawning so the waiter
        // inherits it even if the token was moved to another thread.
        check(unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &raw const self.set, std::ptr::null_mut()) })?;
        let (finish, hold) = mpsc::channel::<()>();
        let waiter = thread::Builder::new().name("stop signals".into()).spawn(move || {
            let mut received = 0;
            // SAFETY: both pointers are valid and the signals are blocked on this thread.
            report(check(unsafe { libc::sigwait(&raw const self.set, &raw mut received) }));
            let _ = hold.recv();
        })?;
        Ok(StopWaiter { waiter: Some((finish, waiter)) })
    }
}

pub(crate) struct CommandSignalMask(libc::sigset_t);

impl CommandSignalMask {
    pub(crate) fn apply(&self, command: &mut Command) {
        let mask = self.0;
        // SAFETY: the pre-exec hook uses only async-signal-safe sigprocmask and errno, with a
        // captured mask that stays valid in the child. The parent never unblocks these signals.
        unsafe {
            command.pre_exec(move || {
                if libc::sigprocmask(libc::SIG_SETMASK, &raw const mask, std::ptr::null_mut()) == 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            });
        }
    }
}

/// Ends and joins the signal waiter, even when the run ended before receiving a signal.
pub struct StopWaiter {
    waiter: Option<(mpsc::Sender<()>, thread::JoinHandle<()>)>,
}

impl Drop for StopWaiter {
    fn drop(&mut self) {
        if let Some((finish, waiter)) = self.waiter.take() {
            // SAFETY: the thread stays alive until finish is dropped. The stop signal is
            // directed only to that thread. If it already received a stop, this one stays
            // blocked and is discarded when the waiter exits.
            let sent = unsafe { libc::pthread_kill(waiter.as_pthread_t(), libc::SIGTERM) };
            drop(finish);
            if sent == 0 {
                let _ = waiter.join();
            }
        }
    }
}

fn check(code: libc::c_int) -> io::Result<()> {
    if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code)) }
}
