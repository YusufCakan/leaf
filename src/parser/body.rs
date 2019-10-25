pub use super::FunctionBuilder;
use crate::evaler::bridge;
use crate::parser::tokenizer::{is_valid_identifier, Key, Operator, RawToken, Token};
use crate::parser::{IdentSource, ParseError, ParseFault};

mod first;
mod r#if;
pub use r#if::IfExpr;
mod list;
mod r#match;
pub use r#match::MatchExpr;
pub mod r#where;

#[derive(PartialEq, Debug)]
pub enum Mode {
    Neutral,
    Operator(Token, Operator),
    Parameters(Vec<Token>),
}

#[derive(Debug)]
pub enum WalkResult {
    Value(Token),
    CloseParen(Option<Token>),
    EOF,
}

pub struct SimpleSource<'a> {
    buf: &'a [Token],
    index: usize,
}

impl<'a> BodySource for SimpleSource<'a> {
    fn next(&mut self) -> Option<Token> {
        let v = self.buf.get(self.index).cloned();
        self.index += 1;
        v
    }
    fn undo(&mut self) {
        self.index -= 1;
    }
}

impl<'a> SimpleSource<'a> {
    pub fn new(buf: &'a [Token]) -> Self {
        Self { index: 0, buf }
    }
}

pub trait BodySource {
    fn next(&mut self) -> Option<Token>;
    fn undo(&mut self);

    fn walk(&mut self, mode: Mode) -> Result<WalkResult, ParseError> {
        let token = match self.next() {
            Some(t) => {
                if t.inner == RawToken::NewLine {
                    return self.walk(mode).map_err(|e| e.fallback(t.source_index));
                } else {
                    t
                }
            }
            None => {
                return match mode {
                    Mode::Parameters(v) => {
                        Ok(WalkResult::Value(Token::new(RawToken::Parameters(v), 0)))
                    }
                    Mode::Operator(left, op) => {
                        ParseFault::EndedMissingRightSideOperator(left.inner, op)
                            .to_err(0)
                            .into()
                    }
                    _ => Ok(WalkResult::EOF),
                }
            }
        };

        match token.inner {
            RawToken::Header(_) | RawToken::Key(Key::Where) => match mode {
                Mode::Operator(left, op) => {
                    ParseFault::MissingRightSideOperator(Box::new((left.inner, op, token.inner)))
                        .to_err(token.source_index)
                        .into()
                }
                Mode::Parameters(previous) => {
                    self.undo();
                    Ok(WalkResult::Value(Token::new(
                        RawToken::Parameters(previous),
                        0,
                    )))
                }
                Mode::Neutral => {
                    self.undo();
                    Ok(WalkResult::EOF)
                }
            },
            RawToken::Key(Key::Pipe) => match mode {
                Mode::Neutral => ParseFault::PipeIntoVoid.to_err(token.source_index).into(),
                Mode::Parameters(mut previous) => {
                    let v = self
                        .walk(Mode::Neutral)
                        .map_err(|e| e.fallback(token.source_index))?;
                    let source = match v {
                        WalkResult::Value(t) => {
                            previous.push(t);
                            previous[0].source_index
                        }
                        WalkResult::EOF => {
                            previous.get(0).map(|a| a.source_index).unwrap_or_else(|| 0)
                        }
                        WalkResult::CloseParen(t) => {
                            return ParseFault::Unexpected(RawToken::Key(Key::ParenClose))
                                .to_err(previous.last().unwrap().source_index)
                                .into();
                        }
                    };
                    Ok(WalkResult::Value(Token::new(
                        RawToken::Parameters(previous),
                        source,
                    )))
                }
                Mode::Operator(left, op) => {
                    let operation = |left: Token, op, right| {
                        let source = left.source_index;
                        Token::new(RawToken::Operation(Box::new((left, right)), op), source)
                    };

                    match self
                        .walk(Mode::Neutral)
                        .map_err(|e| e.fallback(token.source_index))?
                    {
                        WalkResult::Value(t) => {
                            let operation = operation(left, op, t);
                            self.handle_after(operation)
                        }
                        WalkResult::CloseParen(t) => {
                            let t = match t {
                                Some(t) => t,
                                None => {
                                    return ParseFault::MissingRightSideOperator(Box::new((
                                        left.inner,
                                        op,
                                        RawToken::Key(Key::ParenClose),
                                    )))
                                    .to_err(token.source_index)
                                    .into()
                                }
                            };
                            let operation = operation(left, op, t);
                            Ok(WalkResult::CloseParen(Some(operation)))
                        }
                        WalkResult::EOF => ParseFault::MissingRightSideOperator(Box::new((
                            left.inner,
                            op,
                            RawToken::Key(Key::ParenClose),
                        )))
                        .to_err(token.source_index)
                        .into(),
                    }
                }
            },
            RawToken::Key(Key::ParenOpen) => match mode {
                Mode::Neutral => {
                    let v = self
                        .walk(Mode::Neutral)
                        .map_err(|e| e.fallback(token.source_index))?;
                    match v {
                        WalkResult::CloseParen(Some(v)) => self.handle_after(v),
                        WalkResult::CloseParen(None) => {
                            ParseFault::EmptyParen.to_err(token.source_index).into()
                        }
                        _ => ParseFault::Unmatched(Key::ParenClose)
                            .to_err(token.source_index)
                            .into(),
                    }
                }
                Mode::Parameters(mut previous) => {
                    let v = self.walk(Mode::Neutral)?;
                    match v {
                        WalkResult::CloseParen(Some(v)) => {
                            previous.push(v);
                            self.walk(Mode::Parameters(previous))
                        }
                        WalkResult::CloseParen(None) => {
                            ParseFault::EmptyParen.to_err(token.source_index).into()
                        }
                        _ => ParseFault::Unmatched(Key::ParenClose)
                            .to_err(previous.last().unwrap().source_index)
                            .into(),
                    }
                }
                Mode::Operator(left, op) => {
                    let operation = |left: Token, op, right| {
                        let source = left.source_index;
                        Token::new(RawToken::Operation(Box::new((left, right)), op), source)
                    };

                    match self.walk(Mode::Neutral)? {
                        WalkResult::Value(t) => {
                            let operation = operation(left, op, t);
                            self.handle_after(operation)
                        }
                        WalkResult::CloseParen(t) => {
                            let operation = operation(left, op, t.unwrap());
                            self.handle_after(operation)
                        }
                        WalkResult::EOF => ParseFault::MissingRightSideOperator(Box::new((
                            left.inner,
                            op,
                            RawToken::NewLine,
                        )))
                        .to_err(token.source_index)
                        .into(),
                    }
                }
            },
            RawToken::Inlined(v) => {
                let source = token.source_index;
                let reconstruct = Token::new(RawToken::Inlined(v), source);

                match mode {
                    Mode::Neutral => self.handle_after(reconstruct),
                    Mode::Parameters(mut previous) => {
                        previous.push(reconstruct);
                        self.walk(Mode::Parameters(previous))
                    }
                    Mode::Operator(left, op) => {
                        let source = left.source_index;
                        let operation = Token::new(
                            RawToken::Operation(Box::new((left, reconstruct)), op),
                            source,
                        );
                        self.handle_after(operation)
                    }
                }
            }
            RawToken::Identifier(ident) => {
                if !is_valid_identifier(&ident) {
                    return ParseFault::InvalidIdentifier(ident, IdentSource::Ident)
                        .to_err(token.source_index)
                        .into();
                };
                self.handle_ident(
                    mode,
                    Token::new(RawToken::Identifier(ident), token.source_index),
                )
            }
            RawToken::ExternalIdentifier(entries) => {
                if !is_valid_identifier(&entries[0]) {
                    return ParseFault::InvalidIdentifier(entries[0].clone(), IdentSource::Module)
                        .to_err(token.source_index)
                        .into();
                };
                if !is_valid_identifier(&entries[1]) {
                    return ParseFault::InvalidIdentifier(entries[1].clone(), IdentSource::Ident)
                        .to_err(token.source_index)
                        .into();
                };
                let source_index = token.source_index;
                let t = if let Some((bridged_id, bridged_type)) =
                    bridge::try_rust_builtin(&entries).map_err(|e| e.to_err(source_index))?
                {
                    Token::new(
                        RawToken::RustCall(bridged_id, bridged_type),
                        token.source_index,
                    )
                } else {
                    if entries.len() != 2 {
                        return ParseFault::InvalidPath(entries)
                            .to_err(token.source_index)
                            .into();
                    }
                    Token::new(RawToken::ExternalIdentifier(entries), token.source_index)
                };
                self.handle_ident(mode, t)
            }
            RawToken::Key(Key::ListOpen) => {
                let list = list::build(self).map_err(|e| e.fallback(token.source_index))?;
                let v = Token::new(RawToken::List(list), token.source_index);
                match mode {
                    Mode::Neutral => self.handle_after(v),
                    Mode::Parameters(mut previous) => {
                        previous.push(v);
                        self.walk(Mode::Parameters(previous))
                    }
                    Mode::Operator(left, op) => {
                        let operation = Token::new(
                            RawToken::Operation(Box::new((left, v)), op),
                            token.source_index,
                        );
                        self.handle_after(operation)
                    }
                }
            }
            RawToken::Key(Key::First) => {
                let firststm = first::build(self).map_err(|e| e.fallback(token.source_index))?;
                let v = Token::new(RawToken::FirstStatement(firststm), token.source_index);
                match mode {
                    Mode::Neutral => self.handle_after(v),
                    Mode::Parameters(mut previous) => {
                        previous.push(v);
                        self.walk(Mode::Parameters(previous))
                    }
                    Mode::Operator(left, op) => {
                        let operation = Token::new(
                            RawToken::Operation(Box::new((left, v)), op),
                            token.source_index,
                        );
                        self.handle_after(operation)
                    }
                }
            }
            RawToken::Key(Key::If) => {
                let ifexpr = r#if::build(self).map_err(|e| e.fallback(token.source_index))?;
                let v = Token::new(RawToken::IfExpression(ifexpr), token.source_index);
                match mode {
                    Mode::Neutral => self.handle_after(v),
                    Mode::Parameters(mut previous) => {
                        previous.push(v);
                        self.walk(Mode::Parameters(previous))
                    }
                    Mode::Operator(left, op) => {
                        let operation = Token::new(
                            RawToken::Operation(Box::new((left, v)), op),
                            token.source_index,
                        );
                        self.handle_after(operation)
                    }
                }
            }
            RawToken::Key(Key::ParenClose) => match mode {
                Mode::Parameters(previous) => {
                    let t = Token::new(RawToken::Parameters(previous), token.source_index);
                    Ok(WalkResult::CloseParen(Some(t)))
                }
                Mode::Operator(left, op) => {
                    ParseFault::MissingRightSideOperator(Box::new((left.inner, op, token.inner)))
                        .to_err(token.source_index)
                        .into()
                }
                Mode::Neutral => Ok(WalkResult::CloseParen(None)),
            },
            RawToken::Operator(op) => {
                match mode {
                    Mode::Parameters(previous) => {
                        self.undo();
                        let t = Token::new(RawToken::Parameters(previous), token.source_index);
                        Ok(WalkResult::Value(t))
                    }
                    Mode::Operator(left, old_op) => ParseFault::MissingRightSideOperator(Box::new(
                        (left.inner, old_op, RawToken::Operator(op)),
                    ))
                    .to_err(token.source_index)
                    .into(),
                    _ => unimplemented!(), // possible?
                }
            }
            _ => panic!("Unexpected {:?}; MODE:{:?}", token, mode),
        }
    }

    fn handle_after(&mut self, v: Token) -> Result<WalkResult, ParseError> {
        let next = self.next();
        match next.map(|t| t.inner) {
            None => {
                self.undo();
                Ok(WalkResult::Value(v))
            }
            Some(RawToken::Operator(op)) => self.walk(Mode::Operator(v, op)),
            Some(RawToken::NewLine) => self.handle_after(v),
            Some(RawToken::Key(Key::ParenClose)) => Ok(WalkResult::CloseParen(Some(v))),
            _ => {
                self.undo();
                Ok(WalkResult::Value(v))
            }
        }
    }

    fn handle_ident(&mut self, mode: Mode, token: Token) -> Result<WalkResult, ParseError> {
        match mode {
            Mode::Neutral => {
                let want_params = self.walk(Mode::Parameters(Vec::new()))?;
                let source = token.source_index;
                let make_parameterized = |params: Vec<Token>| {
                    if params.is_empty() {
                        token
                    } else {
                        Token::new(RawToken::Parameterized(Box::new(token), params), source)
                    }
                };
                match want_params {
                    WalkResult::Value(v) => match v.inner {
                        RawToken::Parameters(params) => {
                            self.handle_after(make_parameterized(params))
                        }
                        _ => panic!("Wanted parameters but got {:?}", v), // TODO: Should this be an ET?
                    },
                    WalkResult::CloseParen(v) => match v.unwrap().inner {
                        RawToken::Parameters(params) => {
                            Ok(WalkResult::CloseParen(Some(make_parameterized(params))))
                        }
                        _ => panic!("Wanted parameters but got {:?}", ()), // TODO: Should this be an ET?
                    },
                    _ => panic!("Wanted parameters but got {:?}", want_params), // TODO: Should this be an ET?
                }
            }
            Mode::Parameters(mut previous) => {
                previous.push(token);
                self.walk(Mode::Parameters(previous))
            }
            Mode::Operator(left, op) => {
                self.undo();
                let source = left.source_index;
                let v = self.walk(Mode::Neutral)?;
                match v {
                    WalkResult::Value(v) => match &v.inner {
                        RawToken::Parameterized(_n, _p) => {
                            let operation =
                                Token::new(RawToken::Operation(Box::new((left, v)), op), source);
                            self.handle_after(operation)
                        }
                        RawToken::Identifier(_n) => {
                            let operation =
                                Token::new(RawToken::Operation(Box::new((left, v)), op), source);
                            self.handle_after(operation)
                        }
                        _ => panic!("{:?}", &v.inner), // TODO: Should this be an ET?
                    },
                    _ => panic!("{:?}", v), // TODO: Should this be an ET?
                }
            }
        }
    }
}