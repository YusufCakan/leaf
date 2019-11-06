use super::runtime::Runtime;
use crate::ir::{Entity, First, If, Value};

mod bridge;
mod parambuffer;
use parambuffer::*;

pub struct Runner<'a> {
    runtime: &'a Runtime,
    entity: &'a Entity,
    params: ParamBuffer<'a>,
}

impl<'a> Runner<'a> {
    pub fn start(runtime: &'a Runtime, entrypoint: &'a Entity, params: ParamBuffer<'a>) -> Value {
        Self {
            runtime,
            entity: entrypoint,
            params,
        }
        .run()
    }

    fn spawn(&self, entity: &'a Entity, params: ParamBuffer) -> Value {
        Runner {
            runtime: self.runtime,
            entity,
            params,
        }
        .run()
    }

    fn run(mut self) -> Value {
        loop {
            match self.entity {
                Entity::RustCall(index, params) => return self.rust_call(*index, params),
                Entity::Parameter(n) => return self.params.param_consume(*n as usize),
                Entity::Inlined(v) => return v.clone(),
                Entity::IfExpression(expr) => return self.if_expression(expr),
                Entity::FirstStatement(stmt) => return self.first_statement(stmt),
                Entity::FunctionCall(findex, params) => {
                    let evaluated_params = params
                        .iter()
                        .map(|p| self.spawn(p, self.params.borrow()))
                        .collect::<Vec<Value>>()
                        .into();

                    self.params = evaluated_params;
                    let entity = &self.runtime.instructions[*findex as usize];
                    self.entity = entity;
                }
                _ => unimplemented!("{:?}", self.entity),
            }
        }
    }

    fn rust_call(mut self, index: u16, rust_params: &'a [Entity]) -> Value {
        match rust_params.len() {
            2 => {
                let func = bridge::get_func_two(index);

                let a = self.spawn(&rust_params[1], self.params.borrow());

                self.entity = &rust_params[1];
                let b = self.run();

                func(a, b)
            }
            _ => unreachable!(),
        }
    }
    fn if_expression(mut self, expr: &'a If<Entity>) -> Value {
        for i in 0..expr.branches() {
            let cond = expr.condition(i);
            if let Value::Bool(true) = self.spawn(cond, self.params.borrow()) {
                self.entity = expr.evaluation(i);
                return self.run();
                // return self.spawn(eval, self.params.borrow());
            }
        }
        self.entity = expr.r#else();
        self.run()
    }
    fn first_statement(mut self, stmt: &'a First<Entity>) -> Value {
        for entity in stmt.to_void() {
            self.spawn(entity, self.params.borrow());
        }
        self.entity = stmt.to_eval();
        self.run()
    }
}