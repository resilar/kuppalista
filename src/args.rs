use clap::{App, Arg};
use native_tls::Identity;
use rpassword;

use std::env;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;
use std::net::SocketAddr;

/// Parsing errors.
#[derive(Debug)]
pub enum ArgsError {
    BadBindAddress,
    P12DecryptError,
    P12ParseError,
    IoError(io::Error),
}

impl From<io::Error> for ArgsError {
    fn from(error: io::Error) -> Self {
        ArgsError::IoError(error)
    }
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ArgsError::*;
        match *self {
            BadBindAddress => f.write_str("Bad bind address"),
            P12DecryptError => {
                f.write_str("Failed to decrypt PKCS #12 archive, incorrect password?")
            }
            P12ParseError => f.write_str("Failed to parse PKCS #12 archive"),
            IoError(ref err) => write!(f, "{}", err),
        }
    }
}

/// Parsed command-line arguments.
pub struct Args {
    pub pkcs12: Option<Identity>,
    pub addr: SocketAddr,
    pub password: Option<String>,
}

impl Args {
    pub fn parse() -> Result<Args, ArgsError> {
        use self::ArgsError::*;

        let matches = App::new(env!("CARGO_PKG_NAME"))
            .version(env!("CARGO_PKG_VERSION"))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .arg(
                Arg::with_name("address")
                    .help("Sets the address to which the web server will be bound")
                    .value_name("IP:PORT")
                    .required(true),
            )
            .arg(
                Arg::with_name("password")
                    .short("p")
                    .takes_value(true)
                    .help("Password to prevent unwanted access"),
            )
            .arg(
                Arg::with_name("pkcs12")
                    .short("b")
                    .value_name("FILE")
                    .takes_value(true)
                    .help("PKCS #12 bundle for TLS encryption"),
            )
            .arg(
                Arg::with_name("no-decrypt")
                    .long("no-decrypt")
                    .help("Disables asking for password to decrypt PKCS #12 bundle"),
            )
            .get_matches();

        let addr = matches
            .value_of("address")
            .unwrap()
            .parse::<SocketAddr>()
            .map_err(|_| BadBindAddress)?;

        let pkcs12 = match matches.value_of("pkcs12") {
            Some(path) => {
                let mut file = File::open(path)?;
                let mut buffer = vec![];
                file.read_to_end(&mut buffer)?;
                let identity = if !matches.is_present("no-decrypt") {
                    let password = env::var("PKCS12PASS")
                        .or_else(|_| {
                            eprint!("Password for {}: ", path);
                            rpassword::read_password()
                        })
                        .unwrap_or("".to_string());
                    Identity::from_pkcs12(&buffer, &password).map_err(|_| P12DecryptError)
                } else {
                    Identity::from_pkcs12(&buffer, "").map_err(|_| P12ParseError)
                };
                Some(identity?)
            }
            _ => None,
        };

        Ok(Args {
            pkcs12,
            addr,
            password: matches.value_of("password").map(|p| p.to_string()),
        })
    }
}
