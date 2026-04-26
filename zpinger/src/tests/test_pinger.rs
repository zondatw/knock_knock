use super::*;
use std::io::{Error, ErrorKind, Result};
use std::time::Duration;

struct OkPinger;
impl Pinger for OkPinger {
    fn ping(&self) -> Result<()> {
        Ok(())
    }
}

struct ErrPinger;
impl Pinger for ErrPinger {
    fn ping(&self) -> Result<()> {
        Err(Error::other("Test fail"))
    }
}

struct SleepPinger {
    duration: Duration,
}
impl Pinger for SleepPinger {
    fn ping(&self) -> Result<()> {
        std::thread::sleep(self.duration);
        Ok(())
    }
}

#[test]
fn test_timed_ok() {
    let elapsed = timed(&OkPinger).unwrap();
    assert!(elapsed < Duration::from_millis(100));
}

#[test]
fn test_timed_err() {
    assert_eq!(
        Err(ErrorKind::Other),
        timed(&ErrPinger).map_err(|e| e.kind())
    );
}

#[test]
fn test_timed_measures_duration() {
    let p = SleepPinger {
        duration: Duration::from_millis(20),
    };
    let elapsed = timed(&p).unwrap();
    assert!(elapsed >= Duration::from_millis(20));
}

#[test]
fn test_timed_via_dyn_trait() {
    let p: Box<dyn Pinger> = Box::new(OkPinger);
    let elapsed = timed(p.as_ref()).unwrap();
    assert!(elapsed < Duration::from_millis(100));
}
