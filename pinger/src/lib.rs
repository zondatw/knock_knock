use std::collections::HashMap;
use std::io::Result;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

#[path = "tests/test_pinger.rs"]
#[cfg(test)]
mod test_pinger;

pub mod uri;
mod level4;
mod http;

pub use crate::level4::{tcping, udping};
pub use crate::http::{
    httping_connect, httping_get, httping_post, httping_put, httping_delete, httping_patch,
};

pub(crate) const BUF_SIZE: usize = 0xFF;
pub(crate) const HTTP_UNCONNECT_STATUS_CODE: &'static [&'static str] = &["404", "501"];


pub(crate) fn get_host_path(url: &str) -> String {
    let uri = uri::get_uri(url);
    uri.host
}

pub fn resolve(url: &str) -> Vec<SocketAddr> {
    let uri = uri::get_uri(url);
    uri.host.as_str()
        .to_socket_addrs()
        .expect("Unable to resolve domain")
        .collect()
}

type Pinger = fn(&str) -> Result<()>;

pub struct PingHandler {
    pub protocol_map: HashMap<String, Pinger>,
}

impl PingHandler {
    pub fn add_pinger(&mut self, protocol: String, func: Pinger) {
        self.protocol_map.insert(protocol, func);
    }

    pub fn ping(&mut self, protocol: &str, target: &str) -> Result<Duration> {
        let start_time = Instant::now();

        match self.protocol_map[protocol](target) {
            Ok(_) => (),
            Err(err) => return Err(err),
        };

        let elapsed_time = start_time.elapsed();
        Ok(elapsed_time)
    }
}
