use crate::shmem;
use crate::profile_scope;
use rand::Rng as _;

struct ExampleZone
{
    uid: usize,
    name: &'static str
}

const NUM_ZONES: usize = 4;

const EXAMPLE_ZONES: [ExampleZone; NUM_ZONES] = [
    ExampleZone { uid: 61, name: "Example zone 1" },
    ExampleZone { uid: 62, name: "Example zone 2" },
    ExampleZone { uid: 63, name: "Example zone 3" },
    ExampleZone { uid: 64, name: "Example zone 4" }
];

struct TestZoneData {
    uid: usize,
    color: shmem::Color,
    end: shmem::Time,
    duration: shmem::Duration,
    depth: u32,
    name: &'static str,
    copy_strings: bool
}

impl shmem::WriteInto<shmem::ZoneData> for TestZoneData {
    fn write_into(&self, target: &mut shmem::ZoneData) {
        target.uid = self.uid;
        target.color = self.color;
        target.end = self.end;
        target.duration = self.duration;
        target.depth = self.depth;
        target.name.set(self.name, self.copy_strings);
        target.thread.set("thread", self.copy_strings);
    }
}

#[test]
fn test_shmem() {
    let mut mem = shmem::SharedMemory::open().expect("Failed to open shared memory. Make sure the server is actually running.");
    let mut rng = rand::thread_rng();
    let mut already_sent = [false; NUM_ZONES];

    for _ in 0..100 {
        let chosen_zone = rng.gen_range(0, EXAMPLE_ZONES.len());
        let ez = &EXAMPLE_ZONES[chosen_zone];

        let test = TestZoneData {
            uid: ez.uid,
            color: rng.gen(),
            end: rng.gen(),
            duration: rng.gen(),
            depth: rng.gen(),
            name: ez.name,
            copy_strings: !already_sent[chosen_zone]
        };
        
        if mem.zone_data.push(&test) {
            already_sent[chosen_zone] = true;
        }

        let pause = rng.gen_range(0, 100);

        if pause >= 5 {
            std::thread::sleep(std::time::Duration::from_millis(pause));
        }
    }
}

#[test]
fn test_scope_profiling() {
    let mut rng = rand::thread_rng();

    for i in 0..16384 {
        profile_scope!("test_scope");

        if i % 1000 == 0 {
            println!("Sent {} scopes", i);
        }

        let pause = rng.gen_range(0, 10);
        if pause >= 5 {
            std::thread::sleep(std::time::Duration::from_millis(pause));
        } else if pause > 2 {
            std::thread::yield_now();
        }
    }
}
