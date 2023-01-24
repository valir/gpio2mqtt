use libc::{poll, pollfd};
use std::fs::File;
use std::io::{Read, Seek};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::thread;
use std::time::Duration;

const NUM_PINS: usize = 1;
const PINS: [u32; NUM_PINS] = [30];
const PIN_NAMES: [&'static str; NUM_PINS] = ["test"];

fn main() {
    let pin_paths: Vec<String> = PINS
        .iter()
        .map(|pin| format!("/sys/class/gpio/gpio{}/value", pin))
        .collect();
    let mut pin_files: Vec<File> = pin_paths
        .iter()
        .map(|path| File::open(Path::new(path)).unwrap())
        .collect();
    let pin_fds: Vec<i32> = pin_files.iter().map(|file| file.as_raw_fd()).collect();

    let mut fds = vec![
        pollfd {
            fd: 0,
            events: 0,
            revents: 0
        };
        NUM_PINS
    ];
    for (i, &fd) in pin_fds.iter().enumerate() {
        fds[i] = pollfd {
            fd,
            events: libc::POLLPRI,
            revents: 0,
        };
    }

    loop {
        unsafe {
            poll(fds.as_mut_ptr(), fds.len() as u32, -1);
        }

        for (i, fd) in fds.iter().enumerate() {
            if fd.revents & libc::POLLPRI != 0 {
                let mut state = String::new();
                pin_files[i].seek(std::io::SeekFrom::Start(0)).unwrap();
                pin_files[i].read_to_string(&mut state).unwrap();
                println!("{} {}", PIN_NAMES[i], state.trim());
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
}
