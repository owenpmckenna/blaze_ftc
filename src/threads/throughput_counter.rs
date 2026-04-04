use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use atomptr::AtomPtr;

struct ThroughputCounter {
    pub buffer: Vec<AtomPtr<(usize, Instant)>>,
    pub last_valid: usize,
    pub index: AtomicUsize,
    ///allowed bytes per second
    pub allowed: usize,
    pub current_total: AtomicUsize,
    pub valid_time: Duration
}
impl ThroughputCounter {
    fn new(len: usize, valid_time: Duration, allowed: usize) -> ThroughputCounter {
        let buffer = vec![AtomPtr::new((0, Instant::now() - Duration::new(300, 0))); len];
        let tpc = ThroughputCounter {
            buffer,
            last_valid: 0,
            index: AtomicUsize::new(0),
            allowed,
            current_total: AtomicUsize::new(0),
            valid_time,
        };
        //tpc.add(0);
        tpc
    }
}