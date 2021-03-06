use super::ParseFault;
use super::{Anot, Identifier, IdentifierType, Inlinable};
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub mod r#enum;
pub use r#enum::Enum;
pub mod r#struct;
pub use r#struct::Struct;

#[derive(PartialEq, Debug, Clone, Hash, Eq)]
pub enum Type {
    Nothing,
    Int,
    Float,
    Bool,
    Generic(u8),
    List(Box<Type>),
    Struct(i32, i32),
    Function(Box<(Vec<Type>, Type)>),

    // TODO: I'm not sure how to handle this.
    // Because the same type won't cause a match if they're called from different modules.
    // Which of course is wrong. Lets take a look at how they're stored, maybe we can get away with
    // (usize, usize) here instead and evaluate the ast identifier when encountered from it's
    // current scope to discover the actual source of the type. Ye I think that's the way to go!
    // Lets try it.
    //
    // Lets just keep in mind that this would require us to add an extra layer in the error.rs
    // which deserializes the Type::Custom's using the parser.

    // hm no I don't think (usize, usize) will work. Because remember; We store these as
    // encountered declerations, but they might be used before a decleration.
    // Maybe we store it as (FilePath, Identifier<Type>) instead?
    //
    // or `(usize, Identifier<Type>)` makes more sense.
    //
    // I think we're gonna have to create a new type instead of Identifier<Type>. Maybe I want to
    // create an "Annotated<>" type. And then have
    // Identifier<Type>      -> Annotated<Identifier, A=Type>
    // CustomTypeName<Type>  -> Annotated<(usize, String), A=Type>
    //
    // Although how we're gonna get the fid in here I'm not quite sure. Might just need to change
    // TryFrom<&str> to TryFrom<(usize, &str)>.
    Custom(Anot<Identifier, Type>),
    KnownCustom(usize, usize),
}

pub enum CustomType {
    Struct(Struct),
    Enum(Enum),
}
impl fmt::Display for CustomType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CustomType::Enum(a) => write!(f, "{}", a),
            CustomType::Struct(a) => write!(f, "{}", a),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MaybeType {
    Infer(Rc<RefCell<Option<Type>>>),
    Known(Type),
}
impl Default for MaybeType {
    fn default() -> Self {
        Self::new()
    }
}

impl MaybeType {
    pub fn new() -> Self {
        Self::Infer(Rc::default())
    }
    pub fn unwrap(self) -> Type {
        match self {
            MaybeType::Infer(t) => t.borrow().clone().unwrap(),
            MaybeType::Known(t) => t,
        }
    }
}
impl Hash for MaybeType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            MaybeType::Infer(t) => t.borrow().as_ref().unwrap_or(&Type::Nothing).hash(state),
            MaybeType::Known(t) => t.hash(state),
        }
    }
}

impl Type {
    pub fn decoded(self, generics: &HashMap<u8, Type>) -> Self {
        match self {
            Type::Generic(n) => generics.get(&n).cloned().unwrap_or(Type::Generic(n)),
            Type::List(box t) => Type::List(Box::new(t.decoded(generics))),
            Type::Function(attr) => {
                // TODO: Clone can be avoided
                let (mut params, returns) = (attr.0, attr.1);
                params
                    .iter_mut()
                    .for_each(|t| *t = t.clone().decoded(generics));
                Type::Function(Box::new((params, returns.decoded(generics))))
            }
            _ => self,
        }
    }
}

impl std::default::Default for Type {
    fn default() -> Self {
        Type::Nothing
    }
}

impl TryFrom<&str> for Type {
    type Error = ParseFault;

    fn try_from(source: &str) -> Result<Type, Self::Error> {
        if let Some(first) = source.chars().next() {
            // Lists
            if first == '[' {
                if source.len() < 3 {
                    return Err(ParseFault::EmptyListType);
                }
                let inner = source[1..source.len() - 2].trim();
                return Ok(Type::List(Box::new(Type::try_from(inner)?)));
            }
            // Unbound Generics
            if (first as u8) > 96 && (first as u8) < 123 && source.len() == 1 {
                return Ok(Type::Generic(first as u8 - 97));
            }
        } else {
            panic!("Empty type");
        }

        let mut iter = source.chars();
        let mut tbuf = String::new();
        let has_anot = loop {
            match iter.next() {
                Some(c) => {
                    if c == '<' {
                        break true;
                    }
                    tbuf.push(c);
                }
                None => break false,
            };
        };
        let anot = if has_anot {
            annotation(&mut iter).unwrap_or_else(Vec::new)
        } else {
            Vec::new()
        };
        assert_eq!(iter.next(), None);
        let t = match tbuf.as_str() {
            "int" => Type::Int,
            "float" => Type::Float,
            "nothing" | "_" => Type::Nothing,
            "bool" => Type::Bool,
            _ => {
                // TODO: This tbuf.as_str() causes an unessesarry allocation
                Type::Custom(Anot::from((Identifier::try_from(tbuf.as_str())?, anot)))
            }
        };
        Ok(t)
    }
}

pub fn splice_to<I: Iterator<Item = char>>(iter: &mut I, points: &str) -> Option<(char, Type)> {
    let mut s = String::new();
    while let Some(c) = iter.next() {
        if points.contains(|a| a == c) {
            let t = Type::try_from(s.trim()).expect("ET");
            return Some((c, t));
        }
        match c {
            '[' => {
                if !s.is_empty() {
                    panic!("ET: Unexpected [");
                }
                let (a, t) = splice_to(iter, "]")?;
                assert_eq!(a, ']');
                let after = iter.next();
                return Some((after.unwrap_or(a), Type::List(Box::new(t))));
            }
            '<' => {
                let anot = annotation(iter).expect("ET");
                return Some((
                    '>',
                    (Type::Custom(Anot::from((
                        Identifier {
                            path: Vec::new(),
                            name: s,
                            kind: IdentifierType::Normal,
                        },
                        anot,
                    )))),
                ));
            }
            _ => {}
        }
        s.push(c);
    }
    None
}

pub fn annotation<I: Iterator<Item = char>>(iter: &mut I) -> Option<Vec<Type>> {
    let mut annotations = Vec::new();
    loop {
        match splice_to(iter, ",>") {
            Some((was, t)) => match was {
                ',' => {
                    annotations.push(t);
                }
                '>' => {
                    annotations.push(t);
                    if let Some(c) = iter.next() {
                        panic!("ET: Unexpected {}", c);
                    }
                    return Some(annotations);
                }
                _ => unreachable!(),
            },
            None => {
                if annotations.is_empty() {
                    return Some(Vec::new());
                } else {
                    panic!("ET: Annotation missing `>`")
                }
            }
        }
    }
}

impl From<&Inlinable> for Type {
    fn from(v: &Inlinable) -> Type {
        match v {
            Inlinable::Int(_) => Type::Int,
            Inlinable::Float(_) => Type::Float,
            Inlinable::Bool(_) => Type::Bool,
            Inlinable::Nothing => Type::Nothing,
        }
    }
}

impl fmt::Display for MaybeType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MaybeType::Infer(t) => match t.borrow().as_ref() {
                Some(known) => known.fmt(f),
                None => write!(f, "?"),
            },
            MaybeType::Known(known) => known.fmt(f),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Nothing => f.write_str("nothing"),
            Type::Int => f.write_str("int"),
            Type::Float => f.write_str("float"),
            Type::Bool => f.write_str("bool"),
            Type::Generic(gid) => write!(f, "{}", (gid + 97) as char),
            Type::Function(box (takes, gives)) => write!(
                f,
                "({} -> {})",
                takes
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<String>>()
                    .join(" "),
                gives
            ),
            Type::List(inner) => write!(f, "[{}]", inner.to_string()),
            Type::Struct(fid, tid) => write!(f, "Struct({}:{})", fid, tid),
            Type::Custom(name) => write!(f, "unevaluated type {}", name),
            Type::KnownCustom(fid, name) => write!(f, "{}:{}", fid, name),
        }
    }
}
