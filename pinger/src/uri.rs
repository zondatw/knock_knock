use std::io::Result;
use std::io::{Error, ErrorKind};
use regex::Regex;

pub struct URI {
    pub scheme: String,
    pub username: String,
    pub password: String,
    pub host: String,
    pub domain: String,
    pub port: i32,
    pub path: String,
    pub query: String,
    pub fragment: String,
}

impl URI {
    pub fn default() -> URI {
        URI {
            scheme: String::from(""),
            username: String::from(""),
            password: String::from(""),
            host: String::from(""),
            domain: String::from(""),
            port: 0,
            path: String::from("/"),
            query: String::from(""),
            fragment: String::from(""),
        }
    }

    pub fn parse(&mut self, url: &str) -> Result<()> {
        let rg_w_named = Regex::new(r"^((?P<scheme>[^:/?#]+)://)?((?P<username>\w+):(?P<password>\w+)@)?(?P<host>(?P<domain>[^/?#]*)(:(?P<port>\d*)))?(?P<path>[^?#]*)(\?(?P<query>[^#]*))?(#(?P<fragment>.*))?").unwrap(); match rg_w_named.captures(url) {
            Some(uri_parser) => {
                match uri_parser.name("scheme") {
                    Some(v) => self.scheme = v.as_str().to_string(),
                    None => self.scheme = String::from(""),
                }
                match uri_parser.name("host") {
                    Some(v) => self.host = v.as_str().to_string(),
                    None => self.host = String::from(""),
                }
                match uri_parser.name("username") {
                    Some(v) => self.username = v.as_str().to_string(),
                    None => self.username = String::from(""),
                }
                match uri_parser.name("password") {
                    Some(v) => self.password = v.as_str().to_string(),
                    None => self.password = String::from(""),
                }
                match uri_parser.name("domain") {
                    Some(v) => self.domain = v.as_str().to_string(),
                    None => self.domain = String::from(""),
                }
                match uri_parser.name("port") {
                    Some(v) => self.port = v.as_str().to_string().parse::<i32>().unwrap(),
                    None => self.port = 0,
                }
                match uri_parser.name("path") {
                    Some(v) => self.path = v.as_str().to_string(),
                    None => self.path = String::from("/"),
                }
                match uri_parser.name("query") {
                    Some(v) => self.query = v.as_str().to_string(),
                    None => self.query = String::from(""),
                }
                match uri_parser.name("fragment") {
                    Some(v) => self.fragment = v.as_str().to_string(),
                    None => self.fragment = String::from(""),
                }
            },
            None => return Result::Err(Error::new(ErrorKind::InvalidData, "Invalid url")),
        }
        Ok(())
    }

    fn get_url(&mut self) -> String {
        format!(
            "{}://{}:{}@{}{}{}#{}",
            self.scheme,
            self.username,
            self.password,
            self.host,
            self.path,
            self.query,
            self.fragment,
        )
    }

    fn display(&mut self) {
        println!("URI:\n\
             \tScheme: {:?}\n\
             \tUsername: {:?}\n\
             \tPassword: {:?}\n\
             \tHost: {:?}\n\
             \tDomain: {:?}\n\
             \tPort: {:?}\n\
             \tPath: {:?}\n\
             \tQuery: {:?}\n\
             \tFragment: {:?}\n",
             self.scheme,
             self.username,
             self.password,
             self.host,
             self.domain,
             self.port,
             self.path,
             self.query,
             self.fragment,
            );
    }
}

pub fn get_uri(url: &str) -> URI {
    let mut uri = URI::default();
    uri.parse(url);
    uri
}