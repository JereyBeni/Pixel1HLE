//! Linux 3.18 Syscall Translator
//! Low-level POSIX routing engine that intercepts guest bionic libc system calls
//! (epoll, futex, mmap, open, …) and transforms them into native host calls.

use crate::error::{Pixel1Error, Result};
use log::{debug, trace};
use std::collections::HashMap;
use parking_lot::RwLock;

/// Guest file descriptor → host fd / special handle
#[derive(Debug)]
enum FdEntry {
    Host(i32),
    Epoll(u64),   // our own epoll instance id
    EventFd(u64),
    AnonMem(u64), // anonymous mapping id
}

pub struct SyscallTranslator {
    fd_table: RwLock<HashMap<i32, FdEntry>>,
    next_guest_fd: RwLock<i32>,
    // very simple futex wait queue (addr → waiters)
    futex_waiters: RwLock<HashMap<u64, Vec<u64>>>,
}

impl SyscallTranslator {
    pub fn new() -> Self {
        Self {
            fd_table: RwLock::new(HashMap::new()),
            next_guest_fd: RwLock::new(3), // 0/1/2 already open
            futex_waiters: RwLock::new(HashMap::new()),
        }
    }

    /// Main entry: guest issues a syscall number + args
    /// Returns the value that should go into x0 (ARM64)
    pub fn handle(&self, nr: u64, args: [u64; 6]) -> Result<i64> {
        trace!("syscall nr={nr} args={:?}", args);

        match nr {
            // SYS_openat
            56 => self.sys_openat(args[0] as i32, args[1], args[2] as i32, args[3] as i32),
            // SYS_close
            57 => self.sys_close(args[0] as i32),
            // SYS_mmap
            222 => self.sys_mmap(args[0], args[1], args[2] as i32, args[3] as i32, args[4] as i32, args[5]),
            // SYS_munmap
            215 => self.sys_munmap(args[0], args[1]),
            // SYS_futex
            98 => self.sys_futex(args[0], args[1] as i32, args[2] as u32, args[3], args[4], args[5] as u32),
            // SYS_epoll_create1
            20 => self.sys_epoll_create1(args[0] as i32),
            // SYS_epoll_ctl
            21 => self.sys_epoll_ctl(args[0] as i32, args[1] as i32, args[2] as i32, args[3]),
            // SYS_epoll_pwait
            22 => self.sys_epoll_pwait(args[0] as i32, args[1], args[2] as i32, args[3] as i32, args[4]),
            // SYS_clock_gettime
            113 => self.sys_clock_gettime(args[0] as i32, args[1]),
            // SYS_getpid
            172 => Ok(std::process::id() as i64),
            // SYS_exit_group
            94 => {
                debug!("guest called exit_group({})", args[0]);
                std::process::exit(args[0] as i32);
            }
            _ => {
                debug!("unhandled syscall {nr} – returning -ENOSYS");
                Ok(-38) // ENOSYS
            }
        }
    }

    fn alloc_fd(&self, entry: FdEntry) -> i32 {
        let mut next = self.next_guest_fd.write();
        let fd = *next;
        *next += 1;
        self.fd_table.write().insert(fd, entry);
        fd
    }

    fn sys_openat(&self, _dfd: i32, _pathname: u64, _flags: i32, _mode: i32) -> Result<i64> {
        // In full HLE we would resolve the guest path against the sparse storage image.
        // For now we just allocate a dummy fd.
        let fd = self.alloc_fd(FdEntry::Host(-1));
        Ok(fd as i64)
    }

    fn sys_close(&self, fd: i32) -> Result<i64> {
        self.fd_table.write().remove(&fd);
        Ok(0)
    }

    fn sys_mmap(
        &self,
        _addr: u64,
        len: u64,
        _prot: i32,
        _flags: i32,
        _fd: i32,
        _off: u64,
    ) -> Result<i64> {
        // Extremely simplified – real implementation would talk to the CpuRuntime
        // and return a guest virtual address.
        let id = self.alloc_fd(FdEntry::AnonMem(len));
        // fake guest address
        Ok(0x7000_0000_0000 + (id as u64) * 0x10000)
    }

    fn sys_munmap(&self, _addr: u64, _len: u64) -> Result<i64> {
        Ok(0)
    }

    fn sys_futex(
        &self,
        uaddr: u64,
        op: i32,
        val: u32,
        _timeout: u64,
        _uaddr2: u64,
        _val3: u32,
    ) -> Result<i64> {
        // Minimal FUTEX_WAIT / FUTEX_WAKE support
        const FUTEX_WAIT: i32 = 0;
        const FUTEX_WAKE: i32 = 1;

        match op & 0x7f {
            FUTEX_WAIT => {
                // In real code we would spin or park the thread.
                // Here we just record the waiter.
                self.futex_waiters
                    .write()
                    .entry(uaddr)
                    .or_default()
                    .push(val as u64);
                Ok(0)
            }
            FUTEX_WAKE => {
                if let Some(waiters) = self.futex_waiters.write().get_mut(&uaddr) {
                    let woken = waiters.len().min(val as usize);
                    waiters.drain(0..woken);
                    Ok(woken as i64)
                } else {
                    Ok(0)
                }
            }
            _ => Ok(-38), // ENOSYS
        }
    }

    fn sys_epoll_create1(&self, _flags: i32) -> Result<i64> {
        let id = self.alloc_fd(FdEntry::Epoll(0));
        Ok(id as i64)
    }

    fn sys_epoll_ctl(&self, _epfd: i32, _op: i32, _fd: i32, _event: u64) -> Result<i64> {
        Ok(0)
    }

    fn sys_epoll_pwait(
        &self,
        _epfd: i32,
        _events: u64,
        _maxevents: i32,
        _timeout: i32,
        _sigmask: u64,
    ) -> Result<i64> {
        // No events ready
        Ok(0)
    }

    fn sys_clock_gettime(&self, _clk_id: i32, _tp: u64) -> Result<i64> {
        // Real code would write a timespec into guest memory.
        // For now just succeed.
        Ok(0)
    }
}

impl Default for SyscallTranslator {
    fn default() -> Self {
        Self::new()
    }
}
