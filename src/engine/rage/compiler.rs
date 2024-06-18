/*
 Copyright (c) 2024 Michał Wilczek, Michał Margos
 
 Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
 associated documentation files (the “Software”), to deal in the Software without restriction,
 including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense,
 and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do
 so, subject to the following conditions:
 
 The above copyright notice and this permission notice shall be included in all copies or substantial
 portions of the Software.
 
 THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS
 OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
 WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

use std::collections::HashMap;
use std::mem;
use num_traits::ToPrimitive;
use crate::engine::rage::generator::AdjustableTemplate;
use crate::engine::rage::generator::critic::{EvaluateProgram, FigureProgram};
use crate::engine::rage::generator::program::{Instruction, Loc, Program, ValueType};
use crate::engine::rage::generator::program::expr::{AngleBisector, AngleLine, AnglePoint, AnglePointDir, Average, CircleConstruct, EqualComplex, EqualReal, Greater, InvertQuality, Less, LineFromPoints, LineLineIntersection, Max, Negation, ParallelThrough, PartialPow, PartialProduct, PerpendicularThrough, PointLineDistance, PointOnCircle, PointOnLine, PointPointDistance, Sum, SwapParts};
use crate::geometry::{Complex, ValueEnum};
use crate::script::math::{EntityKind, EntityId, Expr, Intermediate, ExprKind, Rule, RuleKind, VarIndex};
use crate::script::token::number::ProcNum;

#[derive(Debug, Default)]
struct Cursor {
    current: Loc
}

impl Cursor {
    fn next(&mut self) -> Loc {
        self.current += 1;
        self.current - 1
    }
}

#[derive(Debug)]
pub struct Compiler<'i> {
    intermediate: &'i Intermediate,
    constants: Vec<ValueEnum>,
    constants_indices: HashMap<ProcNum, usize>,
    cursor: Cursor,
    rule_cursor: Cursor,
    instructions: Vec<Instruction>,
    entities: Vec<Loc>,
    variables: Vec<Loc>,
    alt_mode: bool,
    biases: Vec<Loc>
}

impl<'i> Compiler<'i> {
    #[must_use]
    pub fn new(intermediate: &'i Intermediate) -> Self {
        Self {
            intermediate,
            constants: Vec::new(),
            constants_indices: HashMap::new(),
            cursor: Cursor::default(),
            rule_cursor: Cursor::default(),
            instructions: Vec::new(),
            entities: Vec::new(),
            variables: Vec::new(),
            alt_mode: false,
            biases: Vec::new()
        }
    }

    fn prepare_constants<'r, I: IntoIterator<Item = &'r ExprKind>>(&mut self, adjustable_count: usize, exprs: I) {
        self.constants_indices.clear();
        self.constants.clear();
        self.entities.clear();
        self.constants.resize(adjustable_count, ValueEnum::Complex(Complex::zero()));
        self.entities.resize(adjustable_count, usize::MAX);

        for expr in exprs {
            if let ExprKind::Const { value } = expr {
                self.constants.push(ValueEnum::Complex(value.clone().to_complex()));
                let index = self.constants.len() - 1;
                self.constants_indices.insert(value.clone(), index);
            }
        }
    }

    pub fn compile_programs(mut self) -> (EvaluateProgram, FigureProgram) {
        // 1. Compile `EvaluateProgram`

        // 1a. Figure out adjustables

        let adjustables: Vec<_> = self.intermediate.figure.entities
            .iter()
            .map(|ent| AdjustableTemplate::from(&ent.kind))
            .collect();
        let adj_count = adjustables.len();

        // 1b. Figure out constants

        // The first constants are adjustable values
        self.prepare_constants(adj_count, self.intermediate.adjusted.variables.iter().map(|x| x.kind.as_ref()));

        let rule_count = self.intermediate.adjusted.rules.len();
        let program_zero = self.constants.len() + rule_count;

        // 1c. Compile all instructions

        // Cursor must be placed at the end of constants + rules
        self.cursor.current = program_zero;
        self.rule_cursor.current = self.constants.len();

        self.variables.resize(self.intermediate.adjusted.variables.len(), usize::MAX);

        for rule in &self.intermediate.adjusted.rules {
            self.compile(rule);
        }

        let memory_size = self.cursor.next();

        // 1d. Count weights
        let mut weights = Vec::new();
        weights.resize(rule_count * adjustables.len(), 0.0);

        for (i, rule) in self.intermediate.adjusted.rules.iter().enumerate() {
            for EntityId(adj) in &rule.entities {
                weights[i * adjustables.len() + *adj] = 1.0 / rule.entities.len() as f64;
            }
        }

        let evaluate = EvaluateProgram {
            base: Program {
                req_memory_size: memory_size,
                constants: mem::take(&mut self.constants),
                instructions: mem::take(&mut self.instructions)
            },
            adjustables,
            rule_count,
            biases: mem::take(&mut self.biases),
            weights
        };

        let variable_types: Vec<_> = self.intermediate.figure.variables.iter()
            .map(|expr| Self::get_value_type(&expr.kind))
            .collect();

        let entity_types: Vec<_> = self.intermediate.figure.entities.iter()
            .map(|ent| {
                match &ent.kind {
                    EntityKind::FreeReal
                    | EntityKind::FreePoint
                    | EntityKind::PointOnCircle(_)
                    | EntityKind::PointOnLine(_) => ValueType::Complex,
                    EntityKind::Bind(_) => unreachable!(),
                }
            })
            .collect();

        // 2. Compile `FigureProgram`
        self.variables.clear();
        self.entities.clear();

        // 2a. Figure out constants

        // The first constants are adjustable values
        self.prepare_constants(adj_count, self.intermediate.figure.variables.iter().map(|x| x.kind.as_ref()));

        let program_zero = adjustables.len();
        self.cursor.current = program_zero;

        for expr in &self.intermediate.figure.variables {
            self.compile(expr);
        }

        let figure = FigureProgram {
            base: Program {
                req_memory_size: self.cursor.next(),
                constants: self.constants,
                instructions: self.instructions
            },
            variables: variable_types.into_iter().zip(self.variables).collect(),
            entities: entity_types.into_iter().zip(self.entities).collect()
        };

        (evaluate, figure)
    }

    fn locate_const(&self, constant: &ProcNum) -> Loc {
        self.constants_indices.get(constant).copied().unwrap()
    }

    fn next_rule(&mut self) -> Loc {
        if self.alt_mode {
            self.cursor.next()
        } else {
            self.rule_cursor.next()
        }
    }

    fn set_alt_mode(&mut self, value: bool) {
        self.alt_mode = value;
    }
}

trait Compile<T> {
    fn compile(&mut self, value: &T) -> Loc;
}

impl<'i> Compile<VarIndex> for Compiler<'i> {
    fn compile(&mut self, value: &VarIndex) -> Loc {
        let loc = self.variables[value.0];
        if loc != usize::MAX {
            return loc;
        }

        let loc = self.compile(&self.intermediate.adjusted.variables[value.0]);
        self.variables[value.0] = loc;
        loc
    }
}

impl<'i, M> Compile<Expr<M>> for Compiler<'i> {
    fn compile(&mut self, value: &Expr<M>) -> Loc {
        self.compile(value.kind.as_ref())
    }
}

impl<'i> Compile<ExprKind> for Compiler<'i> {
    fn compile(&mut self, value: &ExprKind) -> Loc {
        match value {
            ExprKind::LineLineIntersection { k, l } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::LineLineIntersection(LineLineIntersection {
                    k: self.compile(k),
                    l: self.compile(l),
                    target
                }));

                target
            }
            ExprKind::AveragePoint { items } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::Average(Average {
                    params: items.iter().map(|i| self.compile(i)).collect(),
                    target
                }));

                target
            }
            ExprKind::CircleCenter { circle } => {
                // It should be enough to just move the circle.
                self.compile(circle)
            }
            ExprKind::Entity { id } => {
                self.compile(id)
            }
            ExprKind::PointPoint { p, q } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::LineFromPoints(LineFromPoints {
                    a: self.compile(p),
                    b: self.compile(q),
                    target
                }));

                target
            }
            ExprKind::AngleBisector { p, q, r } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::AngleBisector(AngleBisector {
                    arm1: self.compile(p),
                    origin: self.compile(q),
                    arm2: self.compile(r),
                    target
                }));

                target
            }
            ExprKind::ParallelThrough { point, line } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::ParallelThrough(ParallelThrough {
                    point: self.compile(point),
                    line: self.compile(line),
                    target
                }));

                target
            }
            ExprKind::PerpendicularThrough { point, line } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::PerpendicularThrough(PerpendicularThrough {
                    point: self.compile(point),
                    line: self.compile(line),
                    target
                }));

                target
            }
            ExprKind::Sum { plus, minus } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::Sum(Sum {
                    params: minus.iter().map(|x| self.compile(x)).collect(),
                    target
                }));

                self.instructions.push(Instruction::Negation(Negation {
                    x: target,
                    target
                }));

                self.instructions.push(Instruction::Sum(Sum {
                    params: Some(target)
                        .into_iter()
                        .chain(plus.iter().map(|x| self.compile(x)))
                        .collect(),
                    target
                }));

                target
            }
            ExprKind::Product { times, by } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::PartialProduct(PartialProduct {
                    params: by.iter().map(|x| self.compile(x)).collect(),
                    target
                }));

                self.instructions.push(Instruction::Pow(PartialPow {
                    value: target,
                    exponent: -1.0,
                    target
                }));

                self.instructions.push(Instruction::PartialProduct(PartialProduct {
                    params: Some(target)
                        .into_iter()
                        .chain(times.iter().map(|x| self.compile(x)))
                        .collect(),
                    target
                }));

                target
            }
            ExprKind::Const { value } => {
                self.locate_const(value)
            }
            ExprKind::Power { value, exponent } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::Pow(PartialPow {
                    value: self.compile(value),
                    exponent: exponent.to_f64().unwrap(),
                    target
                }));

                target
            }
            ExprKind::PointPointDistance { p, q } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::PointPointDistance(PointPointDistance {
                    a: self.compile(p),
                    b: self.compile(q),
                    target
                }));

                target
            }
            ExprKind::PointLineDistance { point, line } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::PointLineDistance(PointLineDistance {
                    point: self.compile(point),
                    line: self.compile(line),
                    target
                }));

                target
            }
            ExprKind::ThreePointAngle { p, q, r } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::AnglePoint(AnglePoint {
                    arm1: self.compile(p),
                    origin: self.compile(q),
                    arm2: self.compile(r),
                    target
                }));

                target
            }
            ExprKind::ThreePointAngleDir { p, q, r } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::AnglePointDir(AnglePointDir {
                    arm1: self.compile(p),
                    origin: self.compile(q),
                    arm2: self.compile(r),
                    target
                }));

                target
            }
            ExprKind::TwoLineAngle { k, l } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::AngleLine(AngleLine {
                    k: self.compile(k),
                    l: self.compile(l),
                    target
                }));

                target
            }
            ExprKind::PointX { point } => {
                // Simply moving is enough.
                self.compile(point)
            }
            ExprKind::PointY { point } => {
                // We're going to have to swap parts, instead of just moving.
                let target = self.cursor.next();

                self.instructions.push(Instruction::SwapParts(SwapParts {
                    x: self.compile(point),
                    target
                }));

                target
            }
            ExprKind::ConstructCircle { center, radius } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::CircleConstruct(CircleConstruct {
                    center: self.compile(center),
                    radius: self.compile(radius),
                    target
                }));

                target
            }
        }
    }
}

impl<'i> Compile<EntityId> for Compiler<'i> {
    fn compile(&mut self, value: &EntityId) -> Loc {
        let loc = self.entities[value.0];
        if loc != usize::MAX {
            return loc;
        }

        let ent = self.intermediate.adjusted.entities[value.0].clone();
        let loc = match ent.kind {
            EntityKind::FreeReal
            | EntityKind::FreePoint => {
                // The first constants are entities.
                value.0
            }
            EntityKind::PointOnLine { line } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::OnLine(PointOnLine {
                    line: self.compile(&line),
                    clip: value.0,
                    target
                }));

                target
            }
            EntityKind::PointOnCircle { circle } => {
                let target = self.cursor.next();

                self.instructions.push(Instruction::OnCircle(PointOnCircle {
                    circle: self.compile(&circle),
                    clip: value.0,
                    target
                }));

                target
            }
            EntityKind::Bind(_) => unreachable!()
        };
        self.entities[value.0] = loc;
        loc
    }
}

impl<'i> Compile<Rule> for Compiler<'i> {
    fn compile(&mut self, value: &Rule) -> Loc {
        self.compile(&value.kind)
    }
}

impl<'i> Compile<RuleKind> for Compiler<'i> {
    fn compile(&mut self, value: &RuleKind) -> Loc {
        match value {
            RuleKind::PointEq(a, b) => {
                let target = self.next_rule();

                self.instructions.push(Instruction::EqualComplex(EqualComplex {
                    a: self.compile(a),
                    b: self.compile(b),
                    target
                }));

                target
            }
            RuleKind::NumberEq(a, b) => {
                let target = self.next_rule();

                self.instructions.push(Instruction::EqualReal(EqualReal {
                    a: self.compile(a),
                    b: self.compile(b),
                    target
                }));

                target
            }
            RuleKind::Lt(a, b) => {
                let target = self.next_rule();

                self.instructions.push(Instruction::Less(Less {
                    a: self.compile(a),
                    b: self.compile(b),
                    target
                }));

                target
            }
            RuleKind::Gt(a, b) => {
                let target = self.next_rule();

                self.instructions.push(Instruction::Greater(Greater {
                    a: self.compile(a),
                    b: self.compile(b),
                    target
                }));

                target
            }
            RuleKind::Alternative(items) => {
                let target = self.next_rule();
                let alt = self.alt_mode;

                if !alt {
                    self.alt_mode = true;
                }

                self.instructions.push(Instruction::MaxReal(Max {
                    params: items.iter().map(|x| self.compile(x)).collect(),
                    target
                }));

                if !alt {
                    self.alt_mode = false;
                }

                target
            }
            RuleKind::Invert(rule) => {
                let target = self.compile(rule.as_ref());

                self.instructions.push(Instruction::InvertQuality(InvertQuality {
                    q: target,
                    target
                }));

                target
            }
            RuleKind::Bias => {
                let index = self.next_rule();
                self.biases.push(index);
                index
            }
        }
    }
}
