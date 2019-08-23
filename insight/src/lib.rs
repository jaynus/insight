use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use crossbeam::queue::ArrayQueue;
use bitflags::bitflags;

pub struct AllocImpl<A> {
    inner: A,
}
impl<A> AllocImpl<A> {
    pub const fn new(inner: A) -> Self {
        Self {
            inner,
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum AllocLog {
    Alloc(Layout),
    Test(Layout, Vec<Option<*mut std::ffi::c_void>>),
    Empty,
}
impl Default for AllocLog {
    fn default() -> Self {
        AllocLog::Empty
    }
}

bitflags! {
    struct AllocFlags: u32 {
        const LOG_DISABLED = 0b00000001;
        const LOG_ENABLED  = 0b00000010;
        const FORBID       = 0b00000100;
    }
}


const ALLOC_LOG_SIZE: usize = 4096;

static mut ALLOC_LOG: Option<ArrayQueue<AllocLog>> = None;
static mut ALLOC_INITIALIZING: AtomicBool = AtomicBool::new(false);

thread_local!(static ALLOC_MODE: Cell<AllocFlags> = Cell::new(AllocFlags::LOG_DISABLED));

pub fn no_log<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
{
    let _guard = Guard::new(AllocFlags::LOG_DISABLED);
    f()
}

pub fn forbid<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
{
    let _guard = Guard::new(AllocFlags::FORBID);
    f()
}

pub struct Guard(AllocFlags);
impl Guard {
    #[inline(always)]
    fn new(newmode: AllocFlags) -> Self {
        ALLOC_MODE.with(|mode| {
            let mut new = mode.get();
            new.insert(newmode);
            mode.set(new);

            Self(newmode)
        })
    }
}

impl Drop for Guard {
    #[inline(always)]
    fn drop(&mut self) {
        ALLOC_MODE.with(|mode| {
            let mut new = mode.get();
            new.remove(self.0);
            mode.set(new);
        });
    }
}

#[inline(always)]
pub unsafe fn add_log_entry(entry: AllocLog) {
    ALLOC_LOG.as_ref().unwrap().push(entry).unwrap();
}

pub unsafe fn dump_alloc() {
    no_log(|| {
        let log = ALLOC_LOG.as_mut().unwrap();
        while ! log.is_empty() {
            match log.pop() {
                Ok(ref entry) => {
                    match entry {
                        AllocLog::Test(ref layout, ref bt) => {
                            let mut trace = Vec::new();
                            bt.iter().for_each(|addr| {
                                if addr.is_some() {
                                    backtrace::resolve(addr.unwrap(), |symbol| {
                                        if symbol.name().is_some() {
                                            let s = symbol.name().unwrap().to_string();
                                            if ! s.contains("backtrace") && ! s.contains("GlobalAlloc") && ! s.contains("insight"){
                                                trace.push(s);
                                            }
                                        }
                                    });
                                }
                            });
                            println!("{:?} - {:?}", layout, trace);
                        },
                        _ => {}
                    }
                },
                Err(e) => panic!("Error: {}", e),
            }
        }
    });
}

unsafe impl<A> GlobalAlloc for AllocImpl<A>
    where
        A: GlobalAlloc,
{
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ALLOC_LOG.is_none() && ! ALLOC_INITIALIZING.load(SeqCst) {
            ALLOC_INITIALIZING.store(true, SeqCst);
            ALLOC_LOG = Some(ArrayQueue::new(ALLOC_LOG_SIZE));
            //ALLOC_MODE.with(|mode| {
            //    mode.set(AllocFlags::LOG_ENABLED);
            //});
        }

        ALLOC_MODE.with(|mode| {
            no_log(|| {
                let mode = mode.get();
                if mode.contains(AllocFlags::FORBID) {
                    panic!("Allocation performed when forbidden")
                }

                if mode.contains(AllocFlags::LOG_ENABLED) {

                    let mut trace = Vec::new();

                    backtrace::trace(|frame| {
                        let ip = frame.ip();
                        backtrace::resolve(ip, |symbol| {
                            trace.push(symbol.addr());
                        });
                        true
                    });

                    add_log_entry(AllocLog::Test(layout, trace));
                }
            });
        });

        self.inner.alloc(layout)
    }

    #[inline(always)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        self.inner.realloc(ptr, layout, new_size)
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.inner.dealloc(ptr, layout)
    }
}

pub type Allocator = AllocImpl<std::alloc::System>;

#[allow(non_upper_case_globals)]
pub const Allocator: Allocator = AllocImpl::new(std::alloc::System);

lazy_static::lazy_static! {
    pub static ref LOG: slog::Logger = create_logger();
}

fn create_logger() -> slog::Logger {
    use slog::Drain;
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    slog::Logger::root(drain, slog::o!())
}

#[cfg(test)]
mod tests {
    #[test]
    fn tracker_macro() {
        println!("Test 1");
    }
}


