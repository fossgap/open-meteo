use std::time::{Duration, Instant};

pub const REFRESH_INTERVALS: [u32; 5] = [5, 15, 30, 60, 0];
pub const DEFAULT_REFRESH_INTERVAL: u32 = 15;

/// Shared cadence for background polling and requests from the GNOME calendar.
pub struct RefreshSchedule {
    interval: Duration,
    next: Option<Instant>,
}

impl RefreshSchedule {
    pub fn new(minutes: u32, now: Instant) -> Self {
        let mut schedule = Self {
            interval: Duration::from_secs(u64::from(minutes) * 60),
            next: None,
        };
        schedule.restart(now);
        schedule
    }

    pub fn restart(&mut self, now: Instant) {
        self.next = (!self.interval.is_zero()).then(|| now + self.interval);
    }

    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.next.is_some_and(|next| now >= next) {
            self.restart(now);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_refresh_follows_each_selected_interval() {
        let now = Instant::now();
        for minutes in [5, 15, 30, 60] {
            let mut schedule = RefreshSchedule::new(minutes, now);
            let due = now + Duration::from_secs(u64::from(minutes) * 60);
            assert!(!schedule.take_due(due - Duration::from_millis(1)));
            assert!(schedule.take_due(due));
            assert!(!schedule.take_due(due));
        }
    }

    #[test]
    fn manual_only_disables_automatic_refresh_even_after_restart() {
        let now = Instant::now();
        let mut schedule = RefreshSchedule::new(0, now);
        assert!(!schedule.take_due(now + Duration::from_secs(86400)));
        schedule.restart(now);
        assert!(!schedule.take_due(now + Duration::from_secs(86400)));
    }

    #[test]
    fn manual_refresh_restarts_the_interval() {
        let now = Instant::now();
        let mut schedule = RefreshSchedule::new(15, now);
        schedule.restart(now + Duration::from_secs(10 * 60));
        assert!(!schedule.take_due(now + Duration::from_secs(15 * 60)));
        assert!(schedule.take_due(now + Duration::from_secs(25 * 60)));
    }

    #[test]
    fn overdue_refresh_does_not_catch_up_with_repeated_requests() {
        let now = Instant::now();
        let mut schedule = RefreshSchedule::new(5, now);
        let later = now + Duration::from_secs(86400);
        assert!(schedule.take_due(later));
        assert!(!schedule.take_due(later));
        assert!(!schedule.take_due(later + Duration::from_secs(1)));
    }
}
