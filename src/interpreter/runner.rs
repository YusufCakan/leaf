use super::runtime::Runtime;
use crate::ir::{bridge::Bridged, Capturable, Entity, First, If, Value};
use std::collections::VecDeque;

mod bridge;
mod parambuffer;
use parambuffer::*;
use termion::color::{Fg, Green, Reset, Yellow};

pub struct Runner<'a> {
    runtime: &'a Runtime,
    entity: &'a Entity,
    params: ParamBuffer<'a>,
    captured: Vec<Value>,
}

#[allow(unused)]
fn debug_dump_entity(runner: &Runner) {
    match runner.entity {
        Entity::Parameter(_) | Entity::Captured(_) | Entity::Inlined(_) | Entity::List(_) => {}
        _ => println!(
            " {g}runner{r} {y}->{r} {g}using{r} ({y}|{r}{}{y}|{r}) {y}({r}{}{y}){r} {g}evaluating{r} {}",
            runner
                .captured
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            runner
                .params
                .as_slice()
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            runner.entity,
            r = Fg(Reset),
            g = Fg(Green),
            y = Fg(Yellow)
        ),
    }
}

impl<'a> Runner<'a> {
    pub fn start(runtime: &'a Runtime, entrypoint: &'a Entity, mut params: Vec<Value>) -> Value {
        Self {
            runtime,
            entity: entrypoint,
            params: ParamBuffer::from(params.drain(0..)),
            captured: Vec::new(),
        }
        .run()
    }

    fn spawn(&self, entity: &'a Entity, params: ParamBuffer, captured: Vec<Value>) -> Value {
        Runner {
            runtime: self.runtime,
            entity,
            params,
            captured,
        }
        .run()
    }

    fn run(mut self) -> Value {
        #[cfg(debug_assertions)]
        debug_dump_entity(&self);

        loop {
            match self.entity {
                Entity::RustCall(index, params) => return self.rust_call(*index, params),
                Entity::Parameter(n) => return self.params.clone_param(*n as usize),
                Entity::Inlined(v) => return v.clone(),
                Entity::IfExpression(expr) => return self.if_expression(expr),
                Entity::FirstStatement(stmt) => return self.first_statement(stmt),
                Entity::List(list) => return self.list(list),
                Entity::ParameterCall(paramid, params) => {
                    let evaluated_params = self.eval_params(params);
                    if let Value::Function(box (entity, captured)) =
                        self.params.clone_param(*paramid as usize)
                    {
                        // TODO: Fix memory management
                        return self.spawn(&entity, evaluated_params, captured);
                    } else {
                        unreachable!();
                    }
                }
                Entity::CapturedCall(capid, params) => {
                    let evaluated_params = self.eval_params(params);
                    if let Value::Function(box (entity, captured)) =
                        self.captured[*capid as usize].clone()
                    {
                        // TODO: Fix memory management
                        return self.spawn(&entity, evaluated_params, captured);
                    } else {
                        unreachable!();
                    }
                }
                Entity::Captured(n) => return self.captured[*n as usize].clone(),
                Entity::Lambda(all, to_capture) => {
                    let entries = &all[1..];
                    let mut buf = Vec::with_capacity(to_capture.len());
                    for c in to_capture.iter() {
                        match c {
                            Capturable::ParentParam(n) => {
                                buf.push(self.params.clone_param(*n).clone())
                            }
                            _ => unimplemented!(),
                        }
                    }

                    self.params = self.eval_params(entries);
                    self.captured = buf;
                    self.entity = &all[0];
                }
                Entity::FunctionCall(findex, params) => {
                    self.params = self.eval_params(params);
                    let entity = &self.runtime.instructions[*findex as usize];
                    self.entity = entity;
                }
                Entity::LambdaPointer(box (inner, to_capture)) => {
                    let mut captured = Vec::with_capacity(to_capture.len());
                    for capturable in to_capture.iter() {
                        match capturable {
                            Capturable::ParentParam(id) => {
                                captured.push(self.params.clone_param(*id).clone())
                            }
                            Capturable::ParentLambda(_id) => {
                                unreachable!()
                            }
                            Capturable::ParentWhere(_) => unimplemented!("`where <identifier>:` values cannot be captured into closures (yet)"),
                        }
                    }
                    return Value::Function(Box::new((inner.clone(), captured)));
                }
                Entity::ConstructRecord(fields) => return self.record(fields),
                Entity::Unimplemented => panic!("TODO: Unimplemented escapes"),
                Entity::Unique => unreachable!(),
            }
        }
    }

    fn eval_params(&mut self, params: &'a [Entity]) -> ParamBuffer<'a> {
        ParamBuffer::from(
            params
                .iter()
                .map(|p| self.spawn(p, self.params.clone(), self.captured.clone())),
        )
    }

    fn record(self, fields: &'a [Entity]) -> Value {
        let mut buf = Vec::with_capacity(fields.len());
        for entity in fields {
            let v = self.spawn(entity, self.params.clone(), self.captured.clone());
            buf.push(v);
        }
        Value::Struct(Box::new(buf))
    }

    fn rust_call(mut self, index: Bridged, rust_params: &'a [Entity]) -> Value {
        self.params = self.eval_params(rust_params);
        self.eval_bridged(index)
    }
    fn if_expression(mut self, expr: &'a If<Entity>) -> Value {
        for i in 0..expr.branches() {
            let cond = expr.condition(i);
            if let Value::Bool(true) = self.spawn(cond, self.params.clone(), self.captured.clone())
            {
                self.entity = expr.evaluation(i);
                return self.run();
            }
        }
        self.entity = expr.r#else();
        self.run()
    }
    fn first_statement(mut self, stmt: &'a First<Entity>) -> Value {
        for entity in stmt.to_void() {
            self.spawn(entity, self.params.clone(), self.captured.clone());
        }
        self.entity = stmt.to_eval();
        self.run()
    }
    fn list(mut self, list: &'a [Entity]) -> Value {
        let mut buf = VecDeque::with_capacity(list.len());
        for entity in list[0..list.len() - 1].iter() {
            buf.push_back(self.spawn(entity, self.params.clone(), self.captured.clone()))
        }
        buf.push_back({
            self.entity = &list[list.len() - 1];
            self.run()
        });
        Value::List(Box::new(buf))
    }
}
