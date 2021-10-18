use super::*;
use std::time::Duration;

fn testping(_target: &str) -> Result<()> {
    Ok(())
}

#[test]
fn test_pinger() {
    let protocol = "Test";
    let mut ping_handler = PingHandler {
        protocol_map: HashMap::new(),
    };
    ping_handler.add_pinger(String::from(protocol), testping);

    assert_eq!(Duration::new(0, 0).as_secs(), ping_handler.ping(protocol, "test").unwrap().as_secs());
}

