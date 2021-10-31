use super::*;
use std::io::{Error, ErrorKind};
use std::time::Duration;

fn testping(_target: &str) -> Result<()> {
    Ok(())
}

fn testping_error(_target: &str) -> Result<()> {
    Result::Err(Error::new(ErrorKind::Other, "Test fail"))
}

#[test]
fn test_get_host_path() {
    assert_eq!(get_host_path("domain.com:80"), "domain.com:80");
    assert_eq!(
        get_host_path("domain.com:80/test/path?param=123#frag"),
        "domain.com:80"
    );
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
