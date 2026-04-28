use super::*;
use async_trait::async_trait;
use std::io::{Error, ErrorKind, Result};
use std::time::Duration;

struct OkPinger;
#[async_trait]
impl Pinger for OkPinger {
    async fn ping(&self) -> Result<()> {
        Ok(())
    }
}

struct ErrPinger;
#[async_trait]
impl Pinger for ErrPinger {
    async fn ping(&self) -> Result<()> {
        Err(Error::other("Test fail"))
    }
}

struct SleepPinger {
    duration: Duration,
}
#[async_trait]
impl Pinger for SleepPinger {
    async fn ping(&self) -> Result<()> {
        tokio::time::sleep(self.duration).await;
        Ok(())
    }
}

#[tokio::test]
async fn test_timed_ok() {
    let elapsed = timed(&OkPinger).await.unwrap();
    assert!(elapsed < Duration::from_millis(100));
}

#[tokio::test]
async fn test_timed_err() {
    assert_eq!(
        Err(ErrorKind::Other),
        timed(&ErrPinger).await.map_err(|e| e.kind())
    );
}

#[tokio::test]
async fn test_timed_measures_duration() {
    let p = SleepPinger {
        duration: Duration::from_millis(20),
    };
    let elapsed = timed(&p).await.unwrap();
    assert!(elapsed >= Duration::from_millis(20));
}

#[tokio::test]
async fn test_timed_via_dyn_trait() {
    let p: Box<dyn Pinger> = Box::new(OkPinger);
    let elapsed = timed(p.as_ref()).await.unwrap();
    assert!(elapsed < Duration::from_millis(100));
}
