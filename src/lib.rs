#[cfg(not(feature = "expose-shmem"))]
mod shmem;

#[cfg(feature = "expose-shmem")]
pub mod shmem;

struct ExampleZone
{
    uid: u32,
    name: &'static str
}

const NUM_ZONES: usize = 4;

const EXAMPLE_ZONES: [ExampleZone; NUM_ZONES] = [
    ExampleZone { uid: 61, name: "Example zone 1" },
    ExampleZone { uid: 62, name: "Example zone 2" },
    ExampleZone { uid: 63, name: "Example zone 3" },
    ExampleZone { uid: 64, name: "Example zone 4" }
];

use rand::Rng;

#[test]
fn test_shmem() {
    let mut mem = shmem::SharedMemory::open().expect("Failed to open shared memory. Make sure the server is actually running.");
    let mut rng = rand::thread_rng();
    let mut already_sent = [false; NUM_ZONES];

    for _ in 0..100 {
        let chosen_zone = rng.gen_range(0, EXAMPLE_ZONES.len());
        let ez = &EXAMPLE_ZONES[chosen_zone];

        let mut test: shmem::ZoneData = unsafe { std::mem::MaybeUninit::uninit().assume_init() };
        test.uid = ez.uid;
        test.color = rng.gen();
        test.start = rng.gen();
        test.duration = rng.gen();
        
        unsafe {
            test.name.set(ez.name, &mut already_sent[chosen_zone]);
        }

        mem.zone_data.push(test);

        let pause = rng.gen_range(0, 100);

        if pause >= 5 {
            std::thread::sleep(std::time::Duration::from_millis(pause));
        }
    }
}
