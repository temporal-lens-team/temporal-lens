#![allow(dead_code)]

#[cfg(not(feature = "expose-shmem"))]
mod shmem;

#[cfg(feature = "expose-shmem")]
pub mod shmem;

struct ZoneDataTmp {
    uid: u32,
    color: shmem::Color,
    start: shmem::Time,
    duration: shmem::Duration,
    name: &'static str,
    copy_name: bool
}

impl shmem::WriteInto<shmem::ZoneData> for ZoneDataTmp {
    fn write_into(&self, target: &mut shmem::ZoneData) {
        target.uid = self.uid;
        target.color = self.color;
        target.start = self.start;
        target.duration = self.duration;
        target.name.set(self.name, self.copy_name);
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng as _;
    use super::{shmem, ZoneDataTmp};

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

    #[test]
    fn test_shmem() {
        let mut mem = shmem::SharedMemory::open().expect("Failed to open shared memory. Make sure the server is actually running.");
        let mut rng = rand::thread_rng();
        let mut already_sent = [false; NUM_ZONES];

        for _ in 0..100 {
            let chosen_zone = rng.gen_range(0, EXAMPLE_ZONES.len());
            let ez = &EXAMPLE_ZONES[chosen_zone];

            let test = ZoneDataTmp {
                uid: ez.uid,
                color: rng.gen(),
                start: rng.gen(),
                duration: rng.gen(),
                name: ez.name,
                copy_name: !already_sent[chosen_zone]
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
}
