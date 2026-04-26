use super::*;
use std::io::{Error, ErrorKind};
use std::time::Duration;

fn testping(_target: &str) -> Result<()> {
    Ok(())
}

fn testping_error(_target: &str) -> Result<()> {
    Result::Err(Error::other("Test fail"))
}

#[test]
fn test_pinger() {
    let protocol = "Test";
    let mut ping_handler = PingHandler {
        protocol_map: HashMap::new(),
    };
    ping_handler.add_pinger(String::from(protocol), testping);

    assert_eq!(
        Duration::new(0, 0).as_secs(),
        ping_handler.ping(protocol, "test").unwrap().as_secs()
    );
}

#[test]
fn test_pinger_error() {
    let protocol = "Test";
    let mut ping_handler = PingHandler {
        protocol_map: HashMap::new(),
    };
    ping_handler.add_pinger(String::from(protocol), testping_error);

    assert_eq!(
        Err(ErrorKind::Other),
        ping_handler.ping(protocol, "test").map_err(|e| e.kind())
    );
}

#[test]
#[should_panic]
fn test_pinger_not_exist() {
    let mut ping_handler = PingHandler {
        protocol_map: HashMap::new(),
    };

    ping_handler.ping("not exist", "test").err();
}

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
