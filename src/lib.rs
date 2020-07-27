//Silent annoying warnings
#![allow(dead_code)]
#![feature(maybe_uninit_extra)]
#![feature(maybe_uninit_ref)]
#![feature(thread_id_value)]

//Imports
use std::time::Instant;
use std::mem::MaybeUninit;
use std::cell::RefCell;
use std::path::PathBuf;
use std::thread_local;

use dirs::data_dir;

//Declare modules
#[cfg(not(feature = "server-mode"))] mod shmem;
#[cfg(feature = "server-mode")] pub mod shmem;
#[cfg(test)] mod tests;
mod core;

pub fn get_data_dir() -> PathBuf {
    let mut ret = data_dir().expect("could not find user data directory");
    ret.push("temporal-lens");

    ret
}

pub struct ThreadInfo {
    id: u64,
    name: String,
    name_sent: bool,
    depth: u32
}

thread_local! {
    static THREAD_INFO: RefCell<Option<ThreadInfo>> = RefCell::new(None);
}

pub struct ZoneInfo {
    color: shmem::Color,
    name: &'static str,
    copy_name: bool
}

impl ZoneInfo {
    pub const fn new(color: shmem::Color, name: &'static str) -> Self {
        Self {
            color, name,
            copy_name: true
        }
    }
}

struct TimeData {
    end: shmem::Time,
    duration: shmem::Duration
}

pub struct Zone {
    info: &'static mut ZoneInfo,
    start: Instant,
    time_data: MaybeUninit<TimeData>,
    thread_id: u64,
    thread_name: Option<(*const u8, usize)>,
    depth: u32
}

impl Zone {
    pub fn new(info: &'static mut ZoneInfo) -> Self {
        let (thread_id, thread_name, depth) = THREAD_INFO.with(|ti| {
            let mut borrowed = ti.borrow_mut();

            if borrowed.is_none() {
                let actual_ti = std::thread::current();

                *borrowed = Some(ThreadInfo {
                    id: actual_ti.id().as_u64().get(),
                    name: actual_ti.name().unwrap_or("").to_string(),
                    name_sent: false,
                    depth: 0
                });
            }

            let ti = borrowed.as_mut().unwrap();
            let depth = ti.depth;

            ti.depth += 1;

            if ti.name_sent {
                (ti.id, None, depth)
            } else {
                let name_bytes = ti.name.as_bytes();
                (ti.id, Some((name_bytes.as_ptr(), name_bytes.len())), depth) //Pointer is fine; we don't plan one changing the name once its set
            }
        });

        let start = Instant::now();

        Self {
            info, start,
            time_data: MaybeUninit::uninit(),
            thread_id, thread_name, depth
        }
    }

    pub fn end(self) {
        //Same as drop(zone)
    }
}

impl shmem::WriteInto<shmem::ZoneData> for Zone {
    fn write_into(&self, target: &mut shmem::ZoneData) {
        target.uid = (self.info as *const ZoneInfo) as usize;
        target.color = self.info.color;
        
        unsafe {
            let time_data = self.time_data.get_ref();

            target.end = time_data.end;
            target.duration = time_data.duration;
            target.depth = self.depth;
            target.name.set(self.info.name, self.info.copy_name);
            target.thread.set_special(self.thread_id as usize, self.thread_name);
        }
    }
}

impl Drop for Zone {
    fn drop(&mut self) {
        let end = Instant::now();

        unsafe {
            //TODO: Maybe we can "cache" shmem and start_time in the THREAD_INFO,
            //which is thread local. This would probably result in faster code.
            let (opt_mem, start_time) = core::get_shmem_data_and_start_time();
            let ok;

            if let Some(mem) = opt_mem {
                self.time_data.write(TimeData {
                    end: end.saturating_duration_since(start_time).as_secs_f64(),
                    duration: end.saturating_duration_since(self.start).as_nanos() as u64
                });

                ok = mem.zone_data.push(self);
            } else {
                ok = false;
            }

            if ok {
                //Name sent; don't need to do it again
                //NOTE: yeah, this is absolutely be thread unsafe,
                //      but we don't care as long as the string is
                //      sent at least once.

                self.info.copy_name = false;
            }

            THREAD_INFO.with(|ti| {
                let mut borrowed = ti.borrow_mut();
                let ti = borrowed.as_mut().unwrap();

                if ok && self.thread_name.is_some() {
                    ti.name_sent = true;
                }

                ti.depth -= 1;
            });
        }
    }
}

#[macro_export]
macro_rules! default_colors {
    (blue)   => { 0x0061afef };
    (orange) => { 0x00d19a66 };
    (purple) => { 0x00c678dd };
    (green)  => { 0x0098c379 };
    (red)    => { 0x00e06c75 };
    (cyan)   => { 0x0056b6c2 };
}

#[macro_export]
macro_rules! start_zone_profiling {
    ($name:literal, color: $color:literal) => {{
        static mut __TL_ZONE_INFO: $crate::ZoneInfo = $crate::ZoneInfo::new($color, $name);
        $crate::Zone::new(unsafe { &mut __TL_ZONE_INFO })
    }};

    ($name:literal, color: $color:ident) => {{
        static mut __TL_ZONE_INFO: $crate::ZoneInfo = $crate::ZoneInfo::new($crate::default_colors!($color), $name);
        $crate::Zone::new(unsafe { &mut __TL_ZONE_INFO })
    }};

    ($name:literal) => {
        $crate::start_zone_profiling!($name, color: orange)
    };
}

#[macro_export]
macro_rules! profile_scope {
    ($name:literal, color: $color:literal) => {
        let __tl_profiling_zone = $crate::start_zone_profiling!($name, color: $color);
    };

    ($name:literal, color: $color:ident) => {
        let __tl_profiling_zone = $crate::start_zone_profiling!($name, color: $color);
    };

    ($name:literal) => {
        $crate::profile_scope!($name, color: orange);
    };
}

pub unsafe fn send_frame_info(num: u64, start: Option<Instant>, end: Instant) {
    let (opt_mem, start_time) = core::get_shmem_data_and_start_time();

    if let Some(mem) = opt_mem {
        let entry = shmem::FrameData {
            number: num,
            end: end.saturating_duration_since(start_time).as_secs_f64(),
            duration: end.saturating_duration_since(start.unwrap_or(start_time)).as_nanos() as u64
        };

        mem.frame_data.push(&entry);
    }
}

#[macro_export]
macro_rules! frame_delimiter {
    () => {{
        static mut __TL_FRAME_TIME: Option<std::time::Instant> = None;
        static mut __TL_FRAME_NUM: u64 = 0;

        unsafe {
            let now = std::time::Instant::now();
            $crate::send_frame_info(__TL_FRAME_NUM, __TL_FRAME_TIME, now);

            __TL_FRAME_TIME = Some(now);
            __TL_FRAME_NUM += 1;
        }
    }}
}

pub fn preinit() {
    unsafe {
        let _ = core::get_shmem_data_and_start_time();
    }
}

#[cfg(feature = "track-heap")]
mod heap_tracker {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use super::shmem::{PlotData, WriteInto};

    struct TLAllocator;
    static SYSTEM_ALLOCATOR: System = System;
    static TOTAL_SIZE: AtomicUsize = AtomicUsize::new(0);

    struct HeapPlotData {
        time: f64,
        value: f64
    }

    impl WriteInto<PlotData> for HeapPlotData {
        fn write_into(&self, target: &mut PlotData) {
            target.time = self.time;
            target.color = 0x0098c379;
            target.value = self.value;
            target.name.set_special(0, None);
        }
    }

    ///Make sure this function never allocates anything, otherwise it goes boom
    unsafe fn report_heap(sz: usize) {
        if let Some((core, start)) = super::core::get_shmem_data_and_start_time_ro() {
            let entry = HeapPlotData {
                time: start.elapsed().as_secs_f64(),
                value: sz as f64,
            };

            core.plot_data.push(&entry);
        }
    }

    unsafe impl GlobalAlloc for TLAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let old = TOTAL_SIZE.fetch_add(layout.size(), Ordering::SeqCst);
            report_heap(old + layout.size());

            SYSTEM_ALLOCATOR.alloc(layout)
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            let old = TOTAL_SIZE.fetch_sub(layout.size(), Ordering::SeqCst);
            report_heap(old - layout.size());

            SYSTEM_ALLOCATOR.dealloc(ptr, layout);
        }
    }

    #[global_allocator]
    static HEAP_TRACKER: TLAllocator = TLAllocator;
}
