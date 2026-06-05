use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct LoginRateLimiter {
    state: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
    max_attempts: u32,
    window: Duration,
}

impl LoginRateLimiter {
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            max_attempts,
            window: Duration::from_secs(window_secs),
        }
    }

    /// Returns `true` if the request is allowed, `false` if it should be rate-limited.
    ///
    /// Each call records a new attempt timestamp. Attempts older than the sliding
    /// window are evicted before the count is checked.
    pub fn check(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let cutoff = now - self.window;
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let attempts = state.entry(ip).or_default();
        while attempts.front().is_some_and(|t| *t < cutoff) {
            attempts.pop_front();
        }
        if attempts.len() as u32 >= self.max_attempts {
            return false;
        }
        attempts.push_back(now);
        true
    }
}
