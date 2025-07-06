use std::time::Instant;

pub struct QuickTimer {
    pub(crate) start: Instant,
    pub(crate) description: String,
}

impl QuickTimer {
    #[must_use]
    pub fn new(description: &str) -> Self {
        QuickTimer {
            start: Instant::now(),
            description: description.to_owned(),
        }
    }

    pub fn add_info(&mut self, info: &str) {
        self.description += info;
    }
}

impl Drop for QuickTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        println!("{:?} !QT! {} ", duration, self.description);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::thread;
    use std::time::Duration;

    #[test]
    fn cpu_timer_instantiates() {
        let timer = QuickTimer::new("Test Timer");
        assert_eq!(timer.description, "Test Timer");
    }

    #[test]
    fn cpu_timer_drops_correctly() {
        // This test checks if QuickTimer can be dropped without errors.
        // Note: This does not check the printed output, as capturing stdout in tests is non-trivial.
        {
            let _timer = QuickTimer::new("Drop Test");
            // Simulate work
            thread::sleep(Duration::from_millis(10));
        }
        // If the test reaches this point without panicking or errors, it's assumed to be successful.
    }
}
