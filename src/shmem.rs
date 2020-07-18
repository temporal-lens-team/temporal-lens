///Description, creation and opening of the shared memory structure used
///to communicate between the server and the app to profile. Note that
///I should have used MaybeUninit everywhere here, but I got really lazy...

use std::sync::atomic::{AtomicBool, Ordering, spin_loop_hint};
use std::thread::yield_now;
use std::path::PathBuf;
use std::ops::Deref;
use std::ops::DerefMut;

use shared_memory::{Shmem, ShmemConf, ShmemError};

#[cfg(feature = "server-mode")]
use serde::{Serialize, Deserialize};

pub const MAGIC: u32 = 0x1DC45EF1;
pub const PROTOCOL_VERSION: u32 = 0x00_01_0004; //Major_Minor_Patch
pub const NUM_ENTRIES: usize = 256;
pub const LOG_DATA_SIZE: usize = 8192;
pub const SHARED_STRING_MAX_SIZE: usize = 128;

pub type Time = f64;     //Low precision time (seconds since program beginning)
pub type Duration = u64; //High precision time difference (nanoseconds)
pub type Color = u32;    //24 bits, 0x00RRGGBB

#[derive(Default)]
struct SpinLock(AtomicBool);

impl SpinLock {
    #[inline]
    fn lock(&self) {
        let mut i = 0;

        while self.0.swap(true, Ordering::Acquire) {
            match i {
                0..=3  => {},
                4..=15 => spin_loop_hint(),
                _      => yield_now()
            }

            i += 1;
        }
    }

    #[inline]
    fn unlock(&self) {
        self.0.store(false, Ordering::Release);
    }
}

pub trait ShouldStopQuery {
    fn should_stop_query(&self, t: f64, query_max: f64) -> bool;
}

#[derive(Copy, Clone)]
pub struct SharedString {
    key: usize,                            //A number that uniquely identifies this zone's name string (typically, the string's address)
    size: u8,                              //The length of this string, max 128 bytes
    has_contents: bool,                    //False if this string has already been sent 
    contents: [u8; SHARED_STRING_MAX_SIZE] //If has_contents is true, the string's contents
}

impl SharedString {
    pub fn set(&mut self, string: &'static str, copy_contents: bool) {
        let raw = string.as_bytes();
        assert!(raw.len() <= SHARED_STRING_MAX_SIZE, "SharedStrings are limited to {} bytes", SHARED_STRING_MAX_SIZE);

        self.key = string.as_ptr() as usize;

        if copy_contents {
            self.size = raw.len() as u8;

            unsafe {
                std::ptr::copy_nonoverlapping(raw.as_ptr(), self.contents.as_mut_ptr(), raw.len());
            }

            self.has_contents = true;
        } else {
            self.has_contents = false;
        }
    }

    pub fn set_special(&mut self, key: usize, contents: Option<(*const u8, usize)>) {
        self.key = key;

        if let Some((raw, sz)) = contents {
            assert!(sz <= SHARED_STRING_MAX_SIZE, "SharedStrings are limited to {} bytes", SHARED_STRING_MAX_SIZE);
            self.size = sz as u8;

            unsafe {
                std::ptr::copy_nonoverlapping(raw, self.contents.as_mut_ptr(), sz);
            }

            self.has_contents = true;
        } else {
            self.has_contents = false;
        }
    }

    #[inline]
    pub fn get_key(&self) -> usize {
        self.key
    }

    #[inline]
    pub fn make_str(&self) -> Option<&str> {
        if self.has_contents {
            Some(unsafe { std::str::from_utf8_unchecked(&self.contents[0..self.size as usize]) })
        } else {
            None
        }
    }

    #[inline]
    pub fn has_contents(&self) -> bool {
        self.has_contents
    }
}

#[derive(Copy, Clone)]
#[cfg_attr(feature = "server-mode", derive(Serialize, Deserialize))]
pub struct FrameData {
    pub number: u64,       //Frame number
    pub end: Time,         //Time when the frame ended
    pub duration: Duration //Total frame time. start = end - duration if you convert the units first ;)
}

impl ShouldStopQuery for FrameData {
    fn should_stop_query(&self, t: f64, query_max: f64) -> bool {
        t - (self.duration as f64) * 1e-9 > query_max
    }
}

#[derive(Copy, Clone)]
pub struct ZoneData {
    pub uid: usize,          //A number that uniquely identifies the zone
    pub color: Color,        //The color of the zone
    pub end: Time,           //Time when the zone ended
    pub duration: Duration,  //The execution time. start = end - duration if you convert the units first ;)
    pub depth: u32,          //Call stack depth
    pub name: SharedString,  //The name of the zone
    pub thread: SharedString //Thread thread ID
}

#[derive(Copy, Clone)]
pub struct PlotData {
    pub time: Time,        //Time (X axis)
    pub color: Color,      //Color of the plot
    pub value: f64,        //Value to plot (Y axis)
    pub name: SharedString //Plot name, which is also used as unique identifier
}

#[derive(Copy, Clone)]
pub struct HeapData {
    pub time: Time,   //Time at which the (de)allocation happened
    pub addr: usize,  //Address of the (de)allocated memory
    pub size: usize,  //Size of the (de)allocated memory
    pub is_free: bool //True if the memory was deallocated, false otherwise
}

#[repr(packed)]
#[derive(Copy, Clone)]
pub struct LogEntryHeader {
    pub time: Time,   //Time at which the message was logged
    pub color: Color, //Color of the message
    pub length: usize //Amount of bytes contained in the string
}

pub struct Payload<T: Sized + Copy> {
    lock: SpinLock,        //A simple spin lock based on an AtomicBool
    size: usize,           //How many valid entries are available in `data`
    data: [T; NUM_ENTRIES]
}

pub struct SharedMemoryData {
    //Compatibility fields
    pub magic: u32,
    pub protocol_version: u32,
    pub size_of_usize: u32,

    //Useful data
    pub frame_data: Payload<FrameData>,
    pub zone_data: Payload<ZoneData>,
    pub heap_data: Payload<HeapData>,
    pub plot_data: Payload<PlotData>,

    //Log data; different as it can contain Strings of variable size
    log_data_lock: SpinLock,          //A simple spin lock based on an AtomicBool
    pub log_data_count: u32,          //How many valid log messages are available in `log_data`
    pub log_data: [u8; LOG_DATA_SIZE] //Array of LogEntryHeader followed by `header.length` bytes of log message
}

pub trait WriteInto<T> {
    fn write_into(&self, target: &mut T);
}

impl<T: Copy> WriteInto<T> for T {
    fn write_into(&self, target: &mut T) {
        *target = *self;
    }
}

impl<T: Sized + Copy> Payload<T> {
    unsafe fn init(&mut self) {
        self.lock.unlock(); //Hack to init
        self.size = 0;
        
    }

    pub fn push<U: WriteInto<T>>(&mut self, entry: &U) -> bool {
        let ret;
        self.lock.lock();

        if self.size < NUM_ENTRIES {
            entry.write_into(&mut self.data[self.size]);
            ret = true;
        } else {
            ret = false;
        }

        self.size += 1;
        self.lock.unlock();
        
        ret
    }

    pub unsafe fn retrieve_unchecked(&mut self, dst: *mut T) -> (usize, usize) {
        self.lock.lock();

        let (retrieved, lost) = if self.size <= NUM_ENTRIES {
            (self.size, 0)
        } else {
            (NUM_ENTRIES, self.size - NUM_ENTRIES)
        };

        std::ptr::copy_nonoverlapping(self.data.as_ptr(), dst, retrieved);
        self.size = 0;

        self.lock.unlock();
        (retrieved, lost)
    }

    pub fn retrieve(&mut self, dst: &mut [T]) -> (usize, usize) {
        assert!(dst.len() >= NUM_ENTRIES, "destination slice has an unsufficient size");

        unsafe {
            self.retrieve_unchecked(dst.as_mut_ptr())
        }
    }
}

impl SharedMemoryData {
    unsafe fn init(&mut self) {
        self.magic = MAGIC;
        self.protocol_version = PROTOCOL_VERSION;
        self.size_of_usize = std::mem::size_of::<usize>() as u32;

        self.frame_data.init();
        self.zone_data.init();
        self.heap_data.init();
        self.plot_data.init();

        self.log_data_lock.unlock(); //Init hack
        self.log_data_count = 0;
    }
}

pub struct SharedMemory {
    data: *mut SharedMemoryData,
    handle: Shmem
}

unsafe impl Send for SharedMemory {}

#[derive(Debug)]
pub enum SharedMemoryOpenError {
    ShmemError(ShmemError),
    BadMagic,
    ProtocolMismatch,
    PlatformMismatch
}

impl SharedMemory {
    pub fn get_path() -> PathBuf {
        let mut ret = super::get_data_dir();
        ret.push("shmem");

        ret
    }

    ///Creates and maps the shared memory
    ///
    ///Note that the directory provided by `temporal_lens::get_data_dir()`
    ///must be created prior to calling this function, otherwise it will
    ///just fail.
    pub fn create() -> Result<SharedMemory, ShmemError> {
        let handle = ShmemConf::new()
            .flink(Self::get_path().as_path())
            .size(std::mem::size_of::<SharedMemoryData>())
            .create()?;

        let data = handle.as_ptr() as *mut SharedMemoryData;
        unsafe {
            (*data).init();
        }

        Ok(SharedMemory { data, handle })
    }

    pub fn open() -> Result<SharedMemory, SharedMemoryOpenError> {
        let handle = ShmemConf::new()
            .flink(Self::get_path().as_path())
            .open().map_err(SharedMemoryOpenError::ShmemError)?;

        let data = handle.as_ptr() as *mut SharedMemoryData;
        let data_ref = unsafe { &mut *data };

        if data_ref.magic != MAGIC {
            Err(SharedMemoryOpenError::BadMagic)
        } else if data_ref.protocol_version != PROTOCOL_VERSION {
            Err(SharedMemoryOpenError::ProtocolMismatch)
        } else if data_ref.size_of_usize != std::mem::size_of::<usize>() as u32 {
            //Might happen if the lib was compiled for x86 and the server was compiled for x86_64
            Err(SharedMemoryOpenError::PlatformMismatch)
        } else {
            Ok(SharedMemory { data, handle })
        }
    }
}

impl Deref for SharedMemory {
    type Target = SharedMemoryData;

    fn deref(&self) -> &SharedMemoryData {
        unsafe {
            &*self.data
        }
    }
}

impl DerefMut for SharedMemory {
    fn deref_mut(&mut self) -> &mut SharedMemoryData {
        unsafe {
            &mut *self.data
        }
    }
}
