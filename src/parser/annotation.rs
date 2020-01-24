use super::{IdentSource, ParseFault};
use std::collections::VecDeque;
use std::convert::TryFrom;
use std::fmt;

#[derive(Clone, PartialEq, Hash, Debug, Eq)]
pub struct Anot<T, A> {
    pub inner: T,
    pub anot: Vec<A>,
}

impl<T, A> Anot<T, A> {
    pub fn new(inner: T) -> Self {
        Anot {
            inner,
            anot: Vec::new(),
        }
    }

    pub fn try_map<NT, F>(self, f: F) -> Result<Anot<NT, A>, ParseFault>
    where
        F: FnOnce(T) -> Result<NT, ParseFault>,
    {
        Ok(Anot {
            inner: f(self.inner)?,
            anot: self.anot,
        })
    }

    pub fn try_map_anot<NA, F>(mut self, f: F) -> Result<Anot<T, NA>, ParseFault>
    where
        F: Fn(A) -> Result<NA, ParseFault>,
    {
        let mut anot = Vec::with_capacity(self.anot.len());
        for a in self.anot.drain(0..) {
            anot.push(f(a)?);
        }
        Ok(Anot {
            inner: self.inner,
            anot,
        })
    }
}

impl<T, A> From<(T, Vec<A>)> for Anot<T, A> {
    fn from((inner, anot): (T, Vec<A>)) -> Anot<T, A> {
        Self { inner, anot }
    }
}

impl<T: Default, A> Default for Anot<T, A> {
    fn default() -> Self {
        Self {
            inner: T::default(),
            anot: Vec::new(),
        }
    }
}

impl TryFrom<&str> for Anot<String, String> {
    type Error = ParseFault;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let mut iter = s.chars();
        let mut base = String::new();
        let mut has_anot = false;
        let first = iter.next();

        // Reason we're doing this is because operators and stuff can actually contain `<` without
        // it being an annotation. It just has to *start* with it.
        match first {
            None => return Err(ParseFault::InvalidIdentifier(base, IdentSource::Ident)),
            Some(other) => base.push(other),
        }

        for c in &mut iter {
            if c == '<' {
                has_anot = true;
                break;
            }
            base.push(c);
        }
        let anots = if has_anot {
            let anots = get_anots(&mut iter)?;
            anots.into()
        } else {
            Vec::new()
        };

        Ok(Anot {
            inner: base,
            anot: anots,
        })
    }
}

fn get_anots<I: Iterator<Item = char>>(iter: &mut I) -> Result<VecDeque<String>, ParseFault> {
    let mut this_anot = String::new();
    while let Some(c) = iter.next() {
        match c {
            '>' => {
                let anots = VecDeque::from(vec![this_anot.trim().to_owned()]);
                return Ok(anots);
            }
            ',' => {
                let mut nested = get_anots(iter)?;
                nested.push_front(this_anot.trim().to_owned());
                return Ok(nested);
            }
            c => this_anot.push(c),
        }
    }
    panic!("ET: Unmatched <");
}

impl<T: fmt::Display, A: fmt::Display> fmt::Display for Anot<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.inner)?;
        if !self.anot.is_empty() {
            write!(
                f,
                "<{}>",
                self.anot
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
        }
        Ok(())
    }
}
