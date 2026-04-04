use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::MovingAverage;

pub struct TimingAnalyzer  {
    min: AtomicU32,
    avg: AtomicU32,
    max: AtomicU32,
    rolling_avg: Mutex<MovingAverage<u32>>
}
impl TimingAnalyzer {
    pub fn new() -> TimingAnalyzer {
        TimingAnalyzer {
            min: AtomicU32::new(u32::MAX),
            avg: AtomicU32::new(0),
            max: AtomicU32::new(0),
            rolling_avg: Mutex::new(MovingAverage::new(500)),
        }
    }
    pub fn update(&self, dur: Duration) {
        let millis = dur.subsec_millis();
        let mut ravg = self.rolling_avg.lock().unwrap();
        ravg.update(dur.subsec_millis());
        self.avg.store(ravg.avg(), Ordering::SeqCst);
        if self.min.load(Ordering::SeqCst) > millis {
            self.min.store(millis, Ordering::SeqCst);
        }
        if self.max.load(Ordering::SeqCst) < millis {
            self.max.store(millis, Ordering::SeqCst);
        }
    }
    pub fn to_text(&self) -> String {
        let min = (self.min.load(Ordering::SeqCst) as f64) / 1000.0;
        let max = (self.max.load(Ordering::SeqCst) as f64) / 1000.0;
        let avg = (self.avg.load(Ordering::SeqCst) as f64) / 1000.0;
        format!("min: {}, avg: {}, max: {}", min, avg, max)
    }
}
