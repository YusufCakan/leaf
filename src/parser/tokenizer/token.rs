use crate::parser::{Anot, IdentSource, Identifier, ParseFault, Tracked};
use std::convert::TryFrom;
use std::fmt;

mod header;
pub use header::Header;
mod key;
pub use key::Key;
mod inlined;
pub use inlined::Inlinable;
mod operator;
pub use operator::Operator;

pub type Token = Tracked<RawToken>;

impl TryFrom<&str> for RawToken {
    type Error = ParseFault;

    fn try_from(bytes: &str) -> Result<RawToken, Self::Error> {
        if bytes.is_empty() {
            return Err(ParseFault::InvalidIdentifier(
                String::new(),
                IdentSource::TypeName,
            ));
        }
        if let Ok(t) = Header::try_from(bytes) {
            Ok(RawToken::Header(t))
        } else if let Ok(t) = Key::try_from(bytes) {
            Ok(RawToken::Key(t))
        } else if let Ok(t) = Inlinable::try_from(bytes) {
            Ok(RawToken::Inlined(t))
        } else if bytes == "\n" {
            Ok(RawToken::NewLine)
        } else {
            let anot: Anot<String, String> = Anot::try_from(bytes)?;
            Ok(RawToken::Identifier(
                anot.try_map(|s| Identifier::try_from(s.as_str()))?,
            ))
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Capture {
    param: String,
    param_ids: Vec<usize>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RawToken {
    Identifier(Anot<Identifier, String>),

    Header(Header),
    Key(Key),
    Inlined(Inlinable),
    NewLine,

    // Special marker that automatically is placed on empty function bodies
    // Allows valid type checking but causes runtime crash
    Unimplemented,
}

impl fmt::Display for RawToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use RawToken::*;
        match self {
            Identifier(ident) => write!(f, "{}", ident),
            Header(h) => h.fmt(f),
            Key(key) => key.fmt(f),
            Inlined(inlined) => inlined.fmt(f),
            NewLine => write!(f, "\\n"),
            _ => panic!("TODO: Format {:?}", self),
        }
    }
}

impl Default for RawToken {
    fn default() -> Self {
        RawToken::Unimplemented
    }
}
