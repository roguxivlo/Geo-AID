use std::{collections::HashMap, rc::Rc, sync::Arc};

use crate::{generator::{self, AdjustableTemplate, Flags, DistanceLiterals, Optimizations, expression::{Expression, ExprKind, expr::{PointPointDistance, PointLineDistance, AnglePoint, Literal, LinePoint, ParallelThrough, PerpendicularThrough, SetUnit, Sum, Difference, Product, Quotient, Negation, AngleBisector, AngleLine, LineLineIntersection, Average, Real, FreePoint, PointX, PointY}, Weights}}, span};

use super::{
    figure::Figure,
    parser::{PredefinedType, Type},
    unroll::{
        self, PointMeta, UnrolledExpression, UnrolledExpressionData, UnrolledRule,
        UnrolledRuleKind, Variable, Flag, VariableMeta
    },
    Criteria, CriteriaKind, Error, HashableRc, SimpleUnit, Weighed, token::{Span, Position}, ty, unit,
};

/// Takes the unrolled expression of type `PointCollection` and takes the point at `index`, isolating it out of the entire expression.
fn index_collection(expr: &UnrolledExpression, index: usize) -> &UnrolledExpression {
    match expr.data.as_ref() {
        UnrolledExpressionData::VariableAccess(var) => index_collection(&var.definition, index),
        UnrolledExpressionData::PointCollection(col) => col.get(index).unwrap(),
        UnrolledExpressionData::Boxed(expr) => index_collection(expr, index),
        _ => unreachable!("PointCollection should never be achievable by this expression."),
    }
}

#[must_use]
pub fn fix_distance(expr: UnrolledExpression, power: i8, dst_var: &Rc<Variable>) -> UnrolledExpression {
    let sp = expr.span;
    let t = expr.ty.clone();

    match power.cmp(&0) {
        std::cmp::Ordering::Less => UnrolledExpression {
            weight: 1.0,
            data: Rc::new(UnrolledExpressionData::Divide(
                fix_distance(expr, power + 1, dst_var),
                UnrolledExpression {
                    weight: 1.0,
                    ty: ty::SCALAR,
                    span: sp,
                    data: Rc::new(UnrolledExpressionData::VariableAccess(Rc::clone(dst_var)))
                }
            )),
            ty: t,
            span: sp,
        },
        std::cmp::Ordering::Equal => expr,
        std::cmp::Ordering::Greater => UnrolledExpression {
            weight: 1.0,
            data: Rc::new(UnrolledExpressionData::Multiply(
                fix_distance(expr, power - 1, dst_var),
                UnrolledExpression {
                    weight: 1.0,
                    ty: ty::SCALAR,
                    span: sp,
                    data: Rc::new(UnrolledExpressionData::VariableAccess(Rc::clone(dst_var)))
                }
            )),
            ty: t,
            span: sp,
        },
    }
}

#[allow(clippy::too_many_lines)]
fn compile_expression(
    expr: &UnrolledExpression,
    variables: &mut HashMap<HashableRc<Variable>, CompiledVariable>,
    expressions: &mut HashMap<HashableRc<UnrolledExpressionData>, Arc<Expression>>,
    template: &mut Vec<AdjustableTemplate>,
    dst_var: &Option<Rc<Variable>>
) -> Arc<Expression> {
    // First we have to check if this expression has been compiled already.
    let key = HashableRc::new(Rc::clone(&expr.data));

    if let Some(v) = expressions.get(&key) {
        // If so, we return it.
        return Arc::clone(v);
    }

    // Otherwise we compile.
    let compiled = match expr.data.as_ref() {
        UnrolledExpressionData::VariableAccess(var) => {
            compile_variable(var, variables, expressions, template, dst_var)
                .assume_compiled()
                .unwrap()
        }
        // Critic doesn't support PointCollections, so this code should never be reached.
        UnrolledExpressionData::PointCollection(_) => {
            unreachable!("PointCollection should never be compiled.")
        }
        UnrolledExpressionData::UnrollParameterGroup(_) => {
            unreachable!("UnrollParameterGroup should never be compiled.")
        }
        UnrolledExpressionData::Number(v) => {
            if expr.ty == ty::SCALAR {
                // If a scalar, we treat it as a standard literal.
                Arc::new(Expression::new(ExprKind::Literal(Literal {
                    value: *v,
                    unit: unit::SCALAR
                }), 1.0))
            } else {
                // Otherwise we pretend it's a scalar literal inside a SetUnit.
                compile_expression(
                    &UnrolledExpression {
                        weight: 1.0,
                        ty: expr.ty.clone(),
                        span: expr.span,
                        data: Rc::new(UnrolledExpressionData::SetUnit(
                            UnrolledExpression {
                                weight: 1.0,
                                ty: ty::SCALAR,
                                span: expr.span,
                                data: expr.data.clone()
                            },
                            expr.ty.as_predefined().unwrap().as_scalar().unwrap().as_ref().unwrap().clone()
                        ))
                    },
                    variables,
                    expressions,
                    template,
                    dst_var
                )
            }
        }
        UnrolledExpressionData::FreePoint => {
            let index = template.len();
            template.push(AdjustableTemplate::Point);

            Arc::new(Expression::new(ExprKind::FreePoint(FreePoint {index}), 1.0))
        }
        UnrolledExpressionData::FreeReal => {
            let index = template.len();
            template.push(AdjustableTemplate::Real);

            Arc::new(Expression::new(ExprKind::Real(Real {index}), 1.0))
        }
        UnrolledExpressionData::Boxed(expr) => {
            compile_expression(expr, variables, expressions, template, dst_var)
        }
        UnrolledExpressionData::Parameter(_) => {
            unreachable!("Parameters should never appear in unroll() output.")
        }
        UnrolledExpressionData::IndexCollection(expr, index) => compile_expression(
            index_collection(expr, *index),
            variables,
            expressions,
            template,
            dst_var
        ),
        UnrolledExpressionData::LineFromPoints(p1, p2) => Arc::new(Expression::new(ExprKind::Line(LinePoint {
            a: compile_expression(p1, variables, expressions, template, dst_var),
            b: compile_expression(p2, variables, expressions, template, dst_var),
        }), 1.0)),
        UnrolledExpressionData::ParallelThrough(line, point) => {
            Arc::new(Expression::new(ExprKind::ParallelThrough(ParallelThrough {
                line: compile_expression(line, variables, expressions, template, dst_var),
                point: compile_expression(point, variables, expressions, template, dst_var),
            }), 1.0))
        }
        UnrolledExpressionData::PerpendicularThrough(line, point) => {
            Arc::new(Expression::new(ExprKind::PerpendicularThrough(PerpendicularThrough { 
                line: compile_expression(line, variables, expressions, template, dst_var),
                point: compile_expression(point, variables, expressions, template, dst_var),
            }), 1.0))
        }
        UnrolledExpressionData::SetUnit(expr, unit) => Arc::new(Expression::new(ExprKind::SetUnit(SetUnit {
            value: compile_expression(
                &fix_distance(expr.clone(), unit[SimpleUnit::Distance as usize] - match expr.ty.as_predefined().unwrap().as_scalar().unwrap() {
                    Some(unit) => unit[SimpleUnit::Distance as usize],
                    None => 0
                }, dst_var.as_ref().unwrap()),
                variables,
                expressions,
                template,
                dst_var
            ),
            unit: unit.clone(),
        }), 1.0)),
        UnrolledExpressionData::PointPointDistance(p1, p2) => {
            Arc::new(Expression::new(ExprKind::PointPointDistance(PointPointDistance {
                a: compile_expression(p1, variables, expressions, template, dst_var),
                b: compile_expression(p2, variables, expressions, template, dst_var),
            }), 1.0))
        }
        UnrolledExpressionData::PointLineDistance(p, l) => {
            Arc::new(Expression::new(ExprKind::PointLineDistance(PointLineDistance {
                point: compile_expression(p, variables, expressions, template, dst_var),
                line: compile_expression(l, variables, expressions, template, dst_var),
            }), 1.0))
        }
        UnrolledExpressionData::Negate(expr) => Arc::new(Expression::new(ExprKind::Negation(Negation {
            value: compile_expression(expr, variables, expressions, template, dst_var),
        }), 1.0)),
        UnrolledExpressionData::Add(v1, v2) => Arc::new(Expression::new(ExprKind::Sum(Sum {
            a: compile_expression(v1, variables, expressions, template, dst_var),
            b: compile_expression(v2, variables, expressions, template, dst_var),
        }), 1.0)),
        UnrolledExpressionData::Subtract(v1, v2) => Arc::new(Expression::new(ExprKind::Difference(Difference {
            a: compile_expression(v1, variables, expressions, template, dst_var),
            b: compile_expression(v2, variables, expressions, template, dst_var),
        }), 1.0)),
        UnrolledExpressionData::Multiply(v1, v2) => Arc::new(Expression::new(ExprKind::Product(Product {
            a: compile_expression(v1, variables, expressions, template, dst_var),
            b: compile_expression(v2, variables, expressions, template, dst_var),
        }), 1.0)),
        UnrolledExpressionData::Divide(v1, v2) => Arc::new(Expression::new(ExprKind::Quotient(Quotient {
            a: compile_expression(v1, variables, expressions, template, dst_var),
            b: compile_expression(v2, variables, expressions, template, dst_var),
        }), 1.0)),
        UnrolledExpressionData::ThreePointAngle(v1, v2, v3) => {
            Arc::new(Expression::new(ExprKind::AnglePoint(AnglePoint {
                arm1: compile_expression(v1, variables, expressions, template, dst_var),
                origin: compile_expression(v2, variables, expressions, template, dst_var),
                arm2: compile_expression(v3, variables, expressions, template, dst_var),
            }), 1.0))
        }
        UnrolledExpressionData::AngleBisector(v1, v2, v3) => {
            Arc::new(Expression::new(ExprKind::AngleBisector(AngleBisector {
                arm1: compile_expression(v1, variables, expressions, template, dst_var),
                origin: compile_expression(v2, variables, expressions, template, dst_var),
                arm2: compile_expression(v3, variables, expressions, template, dst_var),
            }), 1.0))
        }
        UnrolledExpressionData::TwoLineAngle(v1, v2) => {
            Arc::new(Expression::new(ExprKind::AngleLine(AngleLine {
                k: compile_expression(v1, variables, expressions, template, dst_var),
                l: compile_expression(v2, variables, expressions, template, dst_var),
            }), 1.0))
        }
        UnrolledExpressionData::LineLineIntersection(v1, v2) => {
            Arc::new(Expression::new(ExprKind::LineLineIntersection(LineLineIntersection {
                k: compile_expression(v1, variables, expressions, template, dst_var),
                l: compile_expression(v2, variables, expressions, template, dst_var),
            }), 1.0))
        }
        UnrolledExpressionData::Average(exprs) => Arc::new(Expression::new(ExprKind::Average(Average {
            items: exprs
                .iter()
                .map(|expr| compile_expression(expr, variables, expressions, template, dst_var))
                .collect(),
        }), 1.0)),
    };

    // We insert for memory.
    expressions.insert(key, Arc::clone(&compiled));
    compiled
}

/// Attempts to compile the variable. If the variable is a `PointCollection`, leaves it unrolled. Otherwise everything is compiled properly.
fn compile_variable(
    var: &Rc<Variable>,
    variables: &mut HashMap<HashableRc<Variable>, CompiledVariable>,
    expressions: &mut HashMap<HashableRc<UnrolledExpressionData>, Arc<Expression>>,
    template: &mut Vec<AdjustableTemplate>,
    dst_var: &Option<Rc<Variable>>
) -> CompiledVariable {
    // We first have to see if the variable already exists.
    let key = HashableRc::new(Rc::clone(var));

    if let Some(v) = variables.get(&key) {
        // So we can return it here.
        return v.clone();
    }

    // And otherwise compile it.
    let compiled = match &var.definition.ty {
        Type::Predefined(PredefinedType::PointCollection(1)) => {
            CompiledVariable::Compiled(compile_expression(
                index_collection(&var.definition, 0),
                variables,
                expressions,
                template,
                dst_var
            ))
        }
        Type::Predefined(PredefinedType::PointCollection(_)) => {
            CompiledVariable::Unrolled(var.definition.clone())
        }
        _ => CompiledVariable::Compiled(compile_expression(
            &var.definition,
            variables,
            expressions,
            template,
            dst_var
        )),
    };

    // We insert for memory
    variables.insert(HashableRc::new(Rc::clone(var)), compiled.clone());
    compiled
}

/// Represents the output of `compile_variable()`.
#[derive(Debug, Clone)]
enum CompiledVariable {
    /// A compiled variable.
    Compiled(Arc<Expression>),
    /// An unrolled variable of type `PointCollection`.
    Unrolled(UnrolledExpression),
}

impl CompiledVariable {
    fn assume_compiled(self) -> Option<Arc<Expression>> {
        if let Self::Compiled(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

fn compile_rules(
    unrolled: Vec<UnrolledRule>,
    variables: &mut HashMap<HashableRc<Variable>, CompiledVariable>,
    expressions: &mut HashMap<HashableRc<UnrolledExpressionData>, Arc<Expression>>,
    template: &mut Vec<AdjustableTemplate>,
    dst_var: &Option<Rc<Variable>>
) -> Vec<Criteria> {
    unrolled
        .into_iter()
        .map(|rule| {
            let lhs = compile_expression(&rule.lhs, variables, expressions, template, dst_var);
            let rhs = compile_expression(&rule.rhs, variables, expressions, template, dst_var);

            let crit = match rule.kind {
                UnrolledRuleKind::Eq => Weighed::one(CriteriaKind::Equal(lhs, rhs)),
                UnrolledRuleKind::Gt => Weighed::one(CriteriaKind::Greater(lhs, rhs)),
                UnrolledRuleKind::Lt => Weighed::one(CriteriaKind::Less(lhs, rhs)),
            };

            if rule.inverted {
                Weighed {
                    object: CriteriaKind::Inverse(Box::new(crit.object)),
                    weight: crit.weight,
                }
            } else {
                crit
            }
        })
        .collect()
}

fn read_flags(flags: &HashMap<String, Flag>) -> Result<Flags, Error> {
    let distance_literals = &flags["distance_literals"];
    
    Ok(Flags {
            optimizations: Optimizations {
                identical_expressions: flags["optimizations"].as_set().unwrap()["identical_expressions"].as_bool().unwrap()
            },
            distance_literals: match distance_literals.as_ident().unwrap().as_str() {
                "none" => DistanceLiterals::None,
                "adjust" => DistanceLiterals::Adjust,
                "solve" => DistanceLiterals::Solve,
                t => return Err(Error::FlagEnumInvalidValue {
                    error_span: distance_literals.get_span().unwrap(),
                    available_values: &["none", "adjust", "solve"],
                    received_value: t.to_string()
                })
            },
            point_bounds: flags["point_bounds"].as_bool().unwrap()
        }
    )
}

/// Compiles the given script.
///
/// # Errors
/// Exact descriptions of errors are in `ScriptError` documentation.
///
/// # Panics
/// Never
pub fn compile(
    input: &str,
    canvas_size: (usize, usize),
) -> Result<(Vec<Criteria>, Figure, Vec<AdjustableTemplate>, generator::Flags), Error> {
    // First, we have to unroll the script.
    let (unrolled, mut context) = unroll::unroll(input)?;

    let flags = read_flags(&context.flags)?;

    // Check if there's a distance literal in variables or rules.
    // In variables
    let are_literals_present = {
        context.variables.values().map(|var| var.definition.has_distance_literal()).find(Option::is_some).flatten()
            .or_else(|| unrolled.iter()
                .map(|rule| rule.lhs.has_distance_literal().or_else(|| rule.rhs.has_distance_literal()))
                .find(Option::is_some).flatten())
    };

    let dst_var = if let Some(at) = are_literals_present {
        match flags.distance_literals {
            DistanceLiterals::Adjust => {
                // To handle adjusted distance, we create a new adjustable variable that will pretend to be the scale.
                Some(Rc::clone(context.variables.entry(String::from("@distance")).or_insert_with(|| Rc::new(Variable {
                    name: String::from("@distance"),
                    definition_span: span!(0, 0, 0, 0),
                    definition: UnrolledExpression {
                        weight: 0.1, // We reduce the weight of distance to reduce its movement.
                        data: Rc::new(UnrolledExpressionData::FreeReal),
                        ty: ty::SCALAR,
                        span: span!(0, 0, 0, 0),
                    },
                    meta: VariableMeta::Scalar,
                }))))
            },
            DistanceLiterals::Solve => return Err(Error::FetureNotSupported {
                error_span: context.flags.get(&String::from("distance_literals")).unwrap().get_span().unwrap(),
                feature_name: "solve_distance"
            }),
            DistanceLiterals::None => return Err(Error::RequiredFlagNotSet {
                flag_name: "distance_literals",
                required_because: at,
                flagdef_span: None,
                available_values: &["adjust", "solve"],
            }),
        }
    } else {
        None
    };

    // Print variables (debugging)
    // for var in context.variables.values() {
    //     println!("let {} = {}", var.name, var.definition);
    // }

    // Print rules (debugging)
    // for rule in &unrolled {
    //     println!("{rule}");
    // }

    let mut variables = HashMap::new();
    let mut expressions = HashMap::new();
    let mut template = Vec::new();

    // We precompile all variables.
    for (_, var) in context.variables {
        compile_variable(&var, &mut variables, &mut expressions, &mut template, &dst_var);
    }

    // And compile the rules
    let mut criteria = compile_rules(unrolled, &mut variables, &mut expressions, &mut template, &dst_var);

    if let Some(dst) = &dst_var {
        // It's worth noting, that assigning a smaller weight will never be enough. We have to also bias the quality.
        criteria.push(Weighed {
            object: CriteriaKind::Bias(compile_variable(dst, &mut variables, &mut expressions, &mut template, &dst_var).assume_compiled().unwrap()),
            weight: 10.0 // The bias.
        });
    }

    // Add standard bounds
    add_bounds(&template, &mut criteria, &flags);

    // Print the compiled (debugging)
    // for rule in &criteria {
    //     println!("{rule:?}");
    // }

    let figure = Figure {
        // We're displaying every variable of type Point
        points: variables
            .into_iter()
            .filter(|(key, _)| {
                matches!(
                    &key.definition.ty,
                    Type::Predefined(PredefinedType::PointCollection(1) | PredefinedType::Point)
                )
            })
            .map(|(key, def)| {
                (
                    def.assume_compiled().unwrap(),
                    key.meta
                        .as_point()
                        .unwrap()
                        .meta
                        .clone()
                        .unwrap_or(PointMeta {
                            letter: 'P',
                            primes: 0,
                            index: None,
                        }),
                )
            })
            .collect(),
        lines: Vec::new(),
        angles: Vec::new(),
        canvas_size,
    };

    Ok((criteria, figure, template, flags))
}

/// Inequality principle and the point plane limit.
fn add_bounds(template: &[AdjustableTemplate], criteria: &mut Vec<Weighed<CriteriaKind>>, flags: &Flags) {
    // Point inequality principle.
    for (i, _) in template.iter().enumerate().filter(|v: _| v.1.is_point()) {
        // For each of the next points, add an inequality rule.
        for (j, _) in template.iter().enumerate().skip(i+1).filter(|v: _| v.1.is_point()) {
            // println!("{i} != {j}");
            criteria.push(Weighed {
                object: CriteriaKind::Inverse(Box::new(CriteriaKind::Equal(
                    Arc::new(Expression {
                        weights: Weights::one_at(i),
                        kind: ExprKind::FreePoint(FreePoint { index: i })
                    }),
                    Arc::new(Expression {
                        weights: Weights::one_at(j),
                        kind: ExprKind::FreePoint(FreePoint { index: j })
                    }),
                ))),
                weight: 1.0
            });
        }

        if flags.point_bounds {
            // For each point, add a rule limiting its range.
            criteria.push(Weighed {
                object: CriteriaKind::Greater(
                    Arc::new(Expression {
                        weights: Weights::one_at(i),
                        kind: ExprKind::PointX(PointX {
                            point: Arc::new(Expression {
                                weights: Weights::one_at(i),
                                kind: ExprKind::FreePoint(FreePoint { index: i })
                            })
                        })
                    }),
                    Arc::new(Expression {
                        weights: Weights::empty(),
                        kind: ExprKind::Literal(Literal { value: 0.0, unit: unit::SCALAR })
                    }),
                ),
                weight: 1.0
            }); // x > 0

            criteria.push(Weighed {
                object: CriteriaKind::Greater(
                    Arc::new(Expression {
                        weights: Weights::one_at(i),
                        kind: ExprKind::PointY(PointY {
                            point: Arc::new(Expression {
                                weights: Weights::one_at(i),
                                kind: ExprKind::FreePoint(FreePoint { index: i })
                            })
                        })
                    }),
                    Arc::new(Expression {
                        weights: Weights::empty(),
                        kind: ExprKind::Literal(Literal { value: 1.0, unit: unit::SCALAR })
                    }),
                ),
                weight: 1.0
            }); // y > 0

            criteria.push(Weighed {
                object: CriteriaKind::Less(
                    Arc::new(Expression {
                        weights: Weights::one_at(i),
                        kind: ExprKind::PointX(PointX {
                            point: Arc::new(Expression {
                                weights: Weights::one_at(i),
                                kind: ExprKind::FreePoint(FreePoint { index: i })
                            })
                        })
                    }),
                    Arc::new(Expression {
                        weights: Weights::empty(),
                        kind: ExprKind::Literal(Literal { value: 1.0, unit: unit::SCALAR })
                    }),
                ),
                weight: 1.0
            }); // x < 1

            criteria.push(Weighed {
                object: CriteriaKind::Less(
                    Arc::new(Expression {
                        weights: Weights::one_at(i),
                        kind: ExprKind::PointY(PointY {
                            point: Arc::new(Expression {
                                weights: Weights::one_at(i),
                                kind: ExprKind::FreePoint(FreePoint { index: i })
                            })
                        })
                    }),
                    Arc::new(Expression {
                        weights: Weights::empty(),
                        kind: ExprKind::Literal(Literal { value: 1.0, unit: unit::SCALAR })
                    }),
                ),
                weight: 1.0
            }); // y < 1
        }
    }
}
