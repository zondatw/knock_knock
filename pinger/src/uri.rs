use regex::Regex;
use std::io::Result;
use std::io::{Error, ErrorKind};

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
        let rg_w_named = Regex::new(r"^((?P<scheme>[^:/?#]+)://)?((?P<username>\w+):(?P<password>\w+)@)?(?P<host>(?P<domain>[^/?#]*)(:(?P<port>\d*)))?(?P<path>[^?#]*)(\?(?P<query>[^#]*))?(#(?P<fragment>.*))?").unwrap();
        match rg_w_named.captures(url) {
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
            }
            None => return Result::Err(Error::new(ErrorKind::InvalidData, "Invalid url")),
        }
        Ok(())
    }

    pub fn get_url(&mut self) -> String {
        let mut url = format!("{}://", self.scheme);
        if (self.username != "" || self.password != "") {
            url = format!("{}{}:{}@", url, self.username, self.password);
        }
        url = format!("{}{}", url, self.host);
        if (self.path != "") {
            url = format!("{}{}", url, self.path);
        }
        if (self.query != "") {
            url = format!("{}?{}", url, self.query);
        }
        if (self.fragment != "") {
            url = format!("{}#{}", url, self.fragment);
        }
        url
    }

    fn display(&mut self) {
        println!(
            "URI:\n\
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

/// Parse url to uri object
/// # Examples
///
/// ```
/// let mut url: &str = "http://admin:password@sub.domain.org:9999/api/haha?name=test&age=18#YOOO";
/// let mut uri_obj = pinger::uri::get_uri(url);
/// assert_eq!(uri_obj.scheme, String::from("http"));
/// assert_eq!(uri_obj.username, String::from("admin"));
/// assert_eq!(uri_obj.password, String::from("password"));
/// assert_eq!(uri_obj.host, String::from("sub.domain.org:9999"));
/// assert_eq!(uri_obj.domain, String::from("sub.domain.org"));
/// assert_eq!(uri_obj.port, 9999);
/// assert_eq!(uri_obj.path, String::from("/api/haha"));
/// assert_eq!(uri_obj.query, String::from("name=test&age=18"));
/// assert_eq!(uri_obj.fragment, String::from("YOOO"));
/// assert_eq!(uri_obj.get_url(), String::from(url));
///
/// url = "http://sub.domain.org:80/";
/// let mut uri_obj = pinger::uri::get_uri(url);
/// assert_eq!(uri_obj.scheme, String::from("http"));
/// assert_eq!(uri_obj.username, String::from(""));
/// assert_eq!(uri_obj.password, String::from(""));
/// assert_eq!(uri_obj.host, String::from("sub.domain.org:80"));
/// assert_eq!(uri_obj.domain, String::from("sub.domain.org"));
/// assert_eq!(uri_obj.port, 80);
/// assert_eq!(uri_obj.path, String::from("/"));
/// assert_eq!(uri_obj.query, String::from(""));
/// assert_eq!(uri_obj.fragment, String::from(""));
/// assert_eq!(uri_obj.get_url(), String::from(url));
/// ```
pub fn get_uri(url: &str) -> URI {
    let mut uri = URI::default();
    uri.parse(url);
    uri
}
