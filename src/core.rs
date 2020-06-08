use crate::shmem;

use std::sync::Mutex;
use std::sync::Once;
use std::mem::MaybeUninit;
use std::time::Instant;

struct Core
{
    mem: MaybeUninit<shmem::SharedMemory>,
    ready: bool,
    last_check: Mutex<Option<Instant>>,
    start_time: Instant
}

static mut CORE: MaybeUninit<Core> = MaybeUninit::uninit();
static CORE_INITIALIZER: Once = Once::new();

pub unsafe fn get_shmem_data_and_start_time() -> (Option<&'static mut shmem::SharedMemoryData>, Instant) {
    //Initialize core
    //---------------
    //What concerns me is that `Once` relies on an atomic boolean, which issues
    //a memory barrier. Ideally we want to avoid these. However, I don't think
    //there is another way to initialize CORE since contains a Mutex (and thus
    //can't just be created in the static declaration). We can't use the usual
    //singleton pattern either since we would also need a static mutex. The
    //ideal solution would be a kind of "library init" but that's just not
    //possible in Rust.

    CORE_INITIALIZER.call_once(|| {
        CORE.write(Core {
            mem: MaybeUninit::uninit(),
            ready: false,
            last_check: Mutex::new(None),
            start_time: Instant::now()
        });
    });

    let core = CORE.get_mut();

    if std::ptr::read_volatile(&core.ready) {
        //Shared mem is already open
        (Some(&mut *core.mem.get_mut()), core.start_time)
    } else {
        //Shared mem might not be open just yet, lock mutex & check again...
        //Here we assume that the mutex issues a memory barrier, which it surely does
        let mut last_check = core.last_check.lock().unwrap();

        if std::ptr::read_volatile(&core.ready) {
            //False alarm, it's open
            (Some(&mut *core.mem.get_mut()), core.start_time)
        } else {
            //Indeed, it's not open
            let now = Instant::now();
            let should_init = last_check.map(|x| now.saturating_duration_since(x).as_secs() >= 10).unwrap_or(true);
            
            if should_init {
                //Try to initialize again
                let mem_result = shmem::SharedMemory::open();

                if let Ok(mem) = mem_result {
                    let ret = core.mem.write(mem);
                    std::ptr::write_volatile(&mut core.ready, true);
                    
                    //Success!!
                    (Some(ret), core.start_time)
                } else {
                    //Init failure; TODO: report this error!!
                    *last_check = Some(now);
                    (None, core.start_time)
                }
            } else {
                //Not yet time for another try
                (None, core.start_time)
            }
        }
    }
}
