use std::{collections::HashMap, fmt::Debug, io::Read};

use bstr::BString;

pub type HeaderMap = HashMap<BString, Vec<BString>>;

pub struct Request<'a> {
    pub(crate) method: String,
    pub(crate) uri: String,
    pub(crate) version: (u32, u32),
    pub(crate) headers: HeaderMap,
    pub(crate) body: Box<dyn Read + 'a>,
}

impl<'a> Debug for Request<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Request {{
\tmethod: {:?}
\turi: {:?}
\tversion: {:?}
\theaders: {:?}
        }}",
            self.method, self.uri, self.version, self.headers
        )
    }
}
