//Silent annoying warnings
#![allow(dead_code)]
#![feature(maybe_uninit_extra)]
#![feature(maybe_uninit_ref)]

//Imports
use std::time::Instant;
use std::mem::MaybeUninit;

//Declare modules
#[cfg(not(feature = "expose-shmem"))] mod shmem;
#[cfg(feature = "expose-shmem")] pub mod shmem;
#[cfg(test)] mod tests;
mod core;

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
    start: shmem::Time,
    duration: shmem::Duration
}

pub struct Zone {
    info: &'static mut ZoneInfo,
    start: Instant,
    time_data: MaybeUninit<TimeData>
}

impl Zone {
    pub fn new(info: &'static mut ZoneInfo) -> Self {
        let start = Instant::now();

        Self {
            info, start,
            time_data: MaybeUninit::uninit()
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

            target.start = time_data.start;
            target.duration = time_data.duration;
            target.name.set(self.info.name, self.info.copy_name);
        }
    }
}

impl Drop for Zone {
    fn drop(&mut self) {
        let end = Instant::now();

        unsafe {
            let (opt_mem, start_time) = core::get_shmem_data_and_start_time();

            if let Some(mem) = opt_mem {
                self.time_data.write(TimeData {
                    start: end.saturating_duration_since(start_time).as_secs_f64(),
                    duration: end.saturating_duration_since(self.start).as_nanos() as u64
                });

                if mem.zone_data.push(self) {
                    //Name sent; don't need to do it again
                    //NOTE: yeah, this is absolutely be thread unsafe,
                    //      but we don't care as long as the string is
                    //      sent at least once.

                    self.info.copy_name = false;
                }
            }
        }
    }
}

#[macro_export(local_inner_macros)]
macro_rules! start_zone_profiling {
    ($name:literal, color: $color:literal) => {{
        static mut __TL_ZONE_INFO: $crate::ZoneInfo = $crate::ZoneInfo::new($color, $name);
        $crate::Zone::new(unsafe { &mut __TL_ZONE_INFO })
    }};

    ($name:literal) => {
        start_zone_profiling!($name, color: 0x0003FCA5)
    };
}

#[macro_export(local_inner_macros)]
macro_rules! profile_scope {
    ($name:literal, color: $color:literal) => {
        let __tl_profiling_zone = start_zone_profiling!($name, color: $color);
    };

    ($name:literal) => {
        profile_scope!($name, color: 0x0003FCA5);
    };
}
