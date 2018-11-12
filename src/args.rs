use native_tls::Identity;
use rpassword;

use std;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;

/// Parsing errors.
#[derive(Debug)]
pub enum ParseError {
    NoArgs,
    BadArg { idx: usize, arg: String, what: String },
    UnusedArg { idx: usize, arg: String },
    IoError(std::io::Error),
    Other(String)
}

impl From<std::io::Error> for ParseError {
    fn from(error: std::io::Error) -> Self {
        ParseError::IoError(error)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use ParseError::*;
        match *self {
            NoArgs => f.write_str("No arguments given (want usage)"),
            BadArg { idx, ref arg, ref what } => write!(f, "{}: '{}' ({})", what, arg, idx),
            UnusedArg { idx, ref arg } => write!(f, "Unused argument: {} ({})", arg, idx),
            IoError(ref err) => write!(f, "IO error: {}", err),
            Other(ref wtf) => f.write_str(&wtf)
        }
    }
}

/// Parsed command-line arguments.
pub struct Args {
    pub pkcs12: Option<Identity>,
    pub addr: SocketAddr,
    pub password: Option<String>
}

impl Args {
    /// Return usage string.
    pub fn usage(arg0: &str) -> String {
        format!("usage: {0} [pkcs12] IP:PORT [password]\n \
                 e.g.: {0} tls.p12 127.0.0.1:8080 hunter2",
                 arg0)
    }

    /// Ugly af. Do not try to understand.
    pub fn parse<I>(mut args: I) -> Result<Args, ParseError> where I: Iterator<Item=String> {
        use ParseError::*;

        let _arg0 = args.next();
        if let Some(arg1) = args.next() {
            if ["-h", "--help"].iter().any(|h| h == &arg1) { return Err(NoArgs); }
            if let Ok(addr) = arg1.parse::<SocketAddr>() {
                let password = args.next();
                if let Some(arg) = args.next() {
                    Err(UnusedArg { idx: 3, arg: arg })
                } else {
                    Ok(Args { pkcs12: None, addr: addr, password: password })
                }
            } else if let Some(arg2) = args.next() {
                let password = args.next();
                if let Some(arg) = args.next() {
                    Err(UnusedArg { idx: 4, arg: arg })
                } else if let Ok(addr) = arg2.parse::<SocketAddr>() {
                    if let Ok(mut file) = File::open(&arg1) {
                        let mut buffer = vec![];
                        file.read_to_end(&mut buffer)?;

                        eprint!("Password for {}: ", arg1);
                        let input = rpassword::read_password()?;
                        if let Ok(pkcs12) = Identity::from_pkcs12(&buffer, input.trim()) {
                            Ok(Args { pkcs12: Some(pkcs12), addr: addr, password: password })
                        } else {
                            Err(Other("Incorrect password for PKCS12 archive".to_string()))
                        }
                    } else {
                        Err(BadArg { idx: 1, arg: arg1, what: "Bad PKCS12 archive".to_string() })
                    }
                } else if File::open(&arg1).is_err() {
                    Err(BadArg { idx: 1, arg: arg1, what: "Bad PKCS12 archive".to_string() })
                } else {
                    Err(BadArg { idx: 2, arg: arg2, what: "Bad bind address".to_string() })
                }
            } else {
                Err(BadArg { idx: 1, arg: arg1, what: "Bad bind address".to_string() })
            }
        } else {
            Err(NoArgs)
        }
    }
}
