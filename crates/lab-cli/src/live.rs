//! Pausing, stepping, throttling and stopping a running simulation from another thread.
//! A gate only decides *when* the next tick happens, never what happens in it.
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

struct Control {
    paused: bool,
    /// Single-step permits granted while paused.
    steps: u64,
    stop: bool,
    /// Upper limit in ticks per second; 0 means as fast as possible.
    speed: u64,
    last: Instant,
}

pub struct Gate {
    control: Mutex<Control>,
    wake: Condvar,
}

impl Gate {
    pub fn new(paused: bool) -> Self {
        Gate { control: Mutex::new(Control { paused, steps: 0, stop: false, speed: 0, last: Instant::now() }), wake: Condvar::new() }
    }

    fn change(&self, edit: impl FnOnce(&mut Control)) {
        edit(&mut self.control.lock().expect("control lock"));
        self.wake.notify_all();
    }

    /// Applies `pause`, `resume`, `step` or `stop`; false for anything else.
    pub fn command(&self, command: &str) -> bool {
        match command {
            "pause" => self.change(|c| c.paused = true),
            "resume" => self.change(|c| (c.paused, c.steps) = (false, 0)),
            "step" => self.change(|c| c.steps += 1),
            "stop" => self.change(|c| c.stop = true),
            _ => return false,
        }
        true
    }

    pub fn set_speed(&self, ticks_per_second: u64) {
        self.change(|c| c.speed = ticks_per_second);
    }
    pub fn paused(&self) -> bool {
        self.control.lock().expect("control lock").paused
    }
    pub fn speed(&self) -> u64 {
        self.control.lock().expect("control lock").speed
    }

    /// Blocks until the next tick may be computed. False means the run was asked to stop.
    pub fn wait(&self) -> bool {
        let mut control = self.control.lock().expect("control lock");
        loop {
            if control.stop {
                return false;
            }
            if control.paused {
                if control.steps > 0 {
                    control.steps -= 1;
                    return true;
                }
                control = self.wake.wait(control).expect("control lock");
                continue;
            }
            if let Some(nanos) = 1_000_000_000u64.checked_div(control.speed) {
                let due = control.last + Duration::from_nanos(nanos);
                let now = Instant::now();
                if now < due {
                    control = self.wake.wait_timeout(control, due - now).expect("control lock").0;
                    continue;
                }
            }
            control.last = Instant::now();
            return true;
        }
    }
}
