use std::sync::atomic::{AtomicBool, Ordering, spin_loop_hint};
use std::thread::yield_now;
use std::path::PathBuf;
use std::ops::Deref;
use std::ops::DerefMut;

use dirs::data_dir;
use shared_memory::{Shmem, ShmemConf, ShmemError};

pub const PROTOCOL_VERSION: u32 = 0x00_01_0000; //Major_Minor_Patch
pub const NUM_ENTRIES: usize = 32;
pub const LOG_DATA_SIZE: usize = 8192;

pub type Time = f64;     //TBD. Low precision time
pub type Duration = u64; //TBD. High precision time difference
pub type Color = u32;    //24 bits, 0x00RRGGBB
pub type SmallRawString = [u8; 128];

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

#[derive(Copy, Clone, Default)]
pub struct SharedString {
    key: usize,                      //A number that uniquely identifies this zone's name string (typically, the string's address)
    size: u8,                        //The length of this string, max 128 bytes
    contents: Option<SmallRawString> //None if this string has already been sent. Otherwise, the string's contents
}

impl SharedString {
    pub unsafe fn set(&mut self, string: &'static str, already_sent: &mut bool) {
        let raw = string.as_bytes();
        self.key = string.as_ptr() as usize;
        self.size = raw.len() as u8;

        let opt_size = std::mem::size_of::<Option<SmallRawString>>() - std::mem::size_of::<SmallRawString>();
        let content_ptr = (&mut self.contents as *mut Option<SmallRawString>) as *mut u8;

        for i in 0..opt_size {
            //Here we could use std::ptr::write_bytes but I suspect the compiler can optimize this better
            //with a simple for loop as Option should be extremely small (opt_size is known at compile time)

            *content_ptr.offset(i as isize) = 0;
        }

        if !*already_sent {
            *content_ptr = 1; //This is terrible as it assumes little-endian and Option::Some == 1; but I don't know any better...

            std::ptr::copy_nonoverlapping(raw.as_ptr(), self.contents.as_mut().unwrap().as_mut_ptr(), raw.len());
            *already_sent = true;
        }
    }

    #[inline]
    pub fn get_key(&self) -> usize {
        self.key
    }

    #[inline]
    pub fn make_string(&self) -> Option<String> {
        self.contents.map(|c| unsafe { std::str::from_utf8_unchecked(&c[0..self.size as usize]).to_string() })
    }

    #[inline]
    pub fn has_contents(&self) -> bool {
        self.contents.is_some()
    }
}

#[derive(Copy, Clone)]
pub struct ZoneData {
    pub uid: u32,           //A number that uniquely identifies the zone
    pub color: Color,       //The color of the zone
    pub start: Time,        //Time when the zone started
    pub duration: Duration, //The execution time
    pub name: SharedString  //The name of the zone
}

#[derive(Copy, Clone)]
pub struct PlotData {
    pub time: Time,
    pub color: Color,
    pub value: f64,
    pub name: SharedString
}

#[derive(Copy, Clone)]
pub struct HeapData {
    pub time: Time,
    pub addr: usize,
    pub size: usize,
    pub is_free: bool
}

#[repr(packed)]
#[derive(Copy, Clone)]
pub struct LogEntryHeader {
    pub time: Time,
    pub color: Color,
    pub length: usize
}

pub struct Payload<T: Sized + Copy> {
    lock: SpinLock,            //A simple spin lock based on an AtomicBool
    pub size: usize,           //How many valid entries are available in `data`
    pub data: [T; NUM_ENTRIES]
}

pub struct SharedMemoryData {
    //Compatibility fields
    pub protocol_version: u32,
    pub size_of_usize: u32,

    //Useful data
    pub zone_data: Payload<ZoneData>,
    pub heap_data: Payload<HeapData>,
    pub plot_data: Payload<PlotData>,

    //Log data; different as it can contain Strings of variable size
    log_data_lock: SpinLock,          //A simple spin lock based on an AtomicBool
    pub log_data_count: u32,          //How many valid log messages are available in `log_data`
    pub log_data: [u8; LOG_DATA_SIZE] //Array of LogEntryHeader followed by `header.length` bytes of log message
}

impl<T: Sized + Copy> Payload<T> {
    unsafe fn init(&mut self) {
        self.lock.unlock(); //Hack to init
        self.size = 0;
        
    }

    pub fn push(&mut self, entry: T) {
        self.lock.lock();

        if self.size < NUM_ENTRIES {
            self.data[self.size] = entry;
        }

        self.size += 1;
        self.lock.unlock();
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
        self.protocol_version = PROTOCOL_VERSION;
        self.size_of_usize = std::mem::size_of::<usize>() as u32;

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

impl SharedMemory {
    pub fn get_path() -> PathBuf {
        let mut ret = data_dir().expect("could not find user data directory");
        ret.push("temporal-lens-shmem");

        ret
    }

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

    pub fn open() -> Result<SharedMemory, ShmemError> {
        let handle = ShmemConf::new()
            .flink(Self::get_path().as_path())
            .open()?;

        let data = handle.as_ptr() as *mut SharedMemoryData;
        Ok(SharedMemory { data, handle })
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
