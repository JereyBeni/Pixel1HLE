//! Linux 3.18 Syscall Translator

use crate::error::Result;
use log::{debug, trace};
use std::collections::HashMap;
use parking_lot::RwLock;

#[derive(Debug)]
enum FdEntry {
    Host(i32),
    Epoll(u64),
    EventFd(u64),
    AnonMem(u64),
}

pub struct SyscallTranslator {
    fd_table: RwLock<HashMap<i32, FdEntry>>,
    next_guest_fd: RwLock<i32>,
    futex_waiters: RwLock<HashMap<u64, Vec<u64>>>,
}

impl SyscallTranslator {
    pub fn new() -> Self {
        Self {
            fd_table: RwLock::new(HashMap::new()),
            next_guest_fd: RwLock::new(3),
            futex_waiters: RwLock::new(HashMap::new()),
        }
    }

    pub fn handle(&self, nr: u64, args: [u64; 6]) -> Result<i64> {
        trace!("syscall nr={nr} args={:?}", args);

        match nr {
            56 => self.sys_openat(args[0] as i32, args[1], args[2] as i32, args[3] as i32),
            57 => self.sys_close(args[0] as i32),
            222 => self.sys_mmap(args[0], args[1], args[2] as i32, args[3] as i32, args[4] as i32, args[5]),
            215 => self.sys_munmap(args[0], args[1]),
            98 => self.sys_futex(args[0], args[1] as i32, args[2] as u32, args[3], args[4], args[5] as u32),
            20 => self.sys_epoll_create1(args[0] as i32),
            21 => self.sys_epoll_ctl(args[0] as i32, args[1] as i32, args[2] as i32, args[3]),
            22 => self.sys_epoll_pwait(args[0] as i32, args[1], args[2] as i32, args[3] as i32, args[4]),
            113 => self.sys_clock_gettime(args[0] as i32, args[1]),
            172 => Ok(std::process::id() as i64),
            94 => {
                debug!("guest called exit_group({})", args[0]);
                std::process::exit(args[0] as i32);
            }
            _ => {
                debug!("unhandled syscall {nr} – returning -ENOSYS");
                Ok(-38)
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
        let id = self.alloc_fd(FdEntry::AnonMem(len));
        Ok((0x7000_0000_0000u64 + (id as u64) * 0x10000) as i64)
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
        const FUTEX_WAIT: i32 = 0;
        const FUTEX_WAKE: i32 = 1;

        match op & 0x7f {
            FUTEX_WAIT => {
                self.futex_waiters.write().entry(uaddr).or_default().push(val as u64);
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
            _ => Ok(-38),
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
        Ok(0)
    }

    fn sys_clock_gettime(&self, _clk_id: i32, _tp: u64) -> Result<i64> {
        Ok(0)
    }
}

impl Default for SyscallTranslator {
    fn default() -> Self {
        Self::new()
    }
}
