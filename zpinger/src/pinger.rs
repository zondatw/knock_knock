use std::io::Result;
use std::time::{Duration, Instant};

use async_trait::async_trait;

/// Trait every protocol implementation provides. `async fn` is wrapped
/// by `async-trait` so the trait stays object-safe — knockknock
/// dispatches via `Box<dyn Pinger>` and that requires dyn-safety.
#[async_trait]
pub trait Pinger: Send + Sync {
    async fn ping(&self) -> Result<()>;
}

/// Time a single ping. Generic over `?Sized` so it accepts both
/// concrete pinger types and `&dyn Pinger`.
pub async fn timed<P: Pinger + ?Sized>(pinger: &P) -> Result<Duration> {
    let start = Instant::now();
    pinger.ping().await?;
    Ok(start.elapsed())
}
