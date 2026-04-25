use std::io::Result;
use std::time::{Duration, Instant};

pub trait Pinger {
    fn ping(&self) -> Result<()>;
}

pub fn timed<P: Pinger + ?Sized>(pinger: &P) -> Result<Duration> {
    let start = Instant::now();
    pinger.ping()?;
    Ok(start.elapsed())
}
