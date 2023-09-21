use std::sync::Arc;
use std::{collections::HashMap, rc::Rc};

use serde::Serialize;

use uuid::Uuid;

use crate::generator::expression::expr::{AngleLine, AnglePoint};
use crate::generator::expression::{LineExpr, PointExpr, ScalarExpr};
use crate::generator::geometry::get_line;
use crate::{
    generator::{
        critic::EvaluationArgs, expression::Expression, expression::Line, geometry, Adjustable,
        Complex, EvaluationError, Flags,
    },
    script::{figure::Figure, unroll, HashableArc},
};

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use crate::{
        drawer,
        generator::{
            expression::{
                expr::{AnglePoint, CenterRadius, FreePoint, LinePoint, Literal},
                CircleExpr, Expression, LineExpr, PointExpr, ScalarExpr,
            },
            Adjustable, Complex,
        },
        script::{figure::Figure, unroll::PointMeta},
    };

    use super::project;

    /// Utility function used in fn `test_project`(), it makes the code below less messy and more readable.
    fn create_point_expr(index: usize) -> Arc<Expression<PointExpr>> {
        Arc::new(Expression::new(PointExpr::Free(FreePoint { index }), 1.0))
    }

    fn create_point_meta(character: char, primes: u8, index: Option<u16>) -> PointMeta {
        PointMeta {
            letter: character,
            primes,
            index,
        }
    }

    #[test]
    fn test_project() {
        let x: u8 = 1;
        let gen_points: [(Adjustable, f64); 3] = [
            (
                Adjustable::Point(Complex {
                    real: 0.3463,
                    imaginary: 0.436,
                }),
                1.0,
            ),
            (
                Adjustable::Point(Complex {
                    real: 0.23,
                    imaginary: 0.87,
                }),
                1.0,
            ),
            (
                Adjustable::Point(Complex {
                    real: 0.312,
                    imaginary: 0.314,
                }),
                1.0,
            ),
        ];

        let fig = Figure {
            points: vec![
                (create_point_expr(0), create_point_meta('A', 0, None)),
                (create_point_expr(1), create_point_meta('B', 0, None)),
                (create_point_expr(2), create_point_meta('C', 0, None)),
            ],
            lines: vec![
                Arc::new(Expression::new(
                    LineExpr::Line(LinePoint {
                        a: create_point_expr(0),
                        b: create_point_expr(1),
                    }),
                    1.0,
                )),
                Arc::new(Expression::new(
                    LineExpr::Line(LinePoint {
                        a: create_point_expr(1),
                        b: create_point_expr(2),
                    }),
                    1.0,
                )),
            ],
            angles: vec![(
                Arc::new(Expression::new(
                    ScalarExpr::AnglePoint(AnglePoint {
                        arm1: create_point_expr(0),
                        origin: create_point_expr(1),
                        arm2: create_point_expr(2),
                    }),
                    1.0,
                )),
                x,
            )],

            segments: vec![
                (create_point_expr(0), create_point_expr(1)),
                (create_point_expr(1), create_point_expr(2)),
            ],
            rays: vec![(create_point_expr(0), create_point_expr(1))],

            circles: vec![Arc::new(Expression::new(
                CircleExpr::CenterRadius(CenterRadius {
                    center: create_point_expr(0),
                    radius: Arc::new(Expression::new(
                        ScalarExpr::Literal(Literal { value: 0.124 }),
                        1.0,
                    )),
                }),
                1.0,
            ))],

            canvas_size: (200, 200),
        };

        let path_latex = PathBuf::from("testoutputs//test.latex");
        let path_svg = PathBuf::from("testoutputs//test.svg");
        let path_json = PathBuf::from("testoutputs//test.json");
        let path_raw = PathBuf::from("testoutputs//test.raw");

        let pr = &project(&fig, &gen_points, &Arc::default()).unwrap();
        drawer::latex::draw(&path_latex, (fig.canvas_size.0, fig.canvas_size.1), pr);
        drawer::svg::draw(&path_svg, (fig.canvas_size.0, fig.canvas_size.1), pr);
        drawer::json::draw(&path_json, (fig.canvas_size.0, fig.canvas_size.1), pr);
        drawer::raw::draw(&path_raw, (fig.canvas_size.0, fig.canvas_size.1), pr);
    }
}

/// Enum representing things that are later drawn in the drawers.
#[derive(Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Rendered {
    Point(Rc<RenderedPoint>),
    Line(RenderedLine),
    Angle(RenderedAngle),
    Segment(RenderedSegment),
    Ray(RenderedRay),
    Circle(RenderedCircle),
}

/// The final product passed to the drawers.
#[derive(Serialize)]
pub struct Output {
    /// Map containing Expression (points) as keys and point structs as values
    pub map: HashMap<HashableArc<Expression<PointExpr>>, Rc<RenderedPoint>>,
    /// final product of the project function
    pub vec_rendered: Vec<Rendered>,
}

#[derive(Debug, Serialize)]
pub struct RenderedPoint {
    /// The point's label
    pub label: String,
    /// Point's position
    pub position: Complex,
    /// Point's custom uuid
    pub uuid: Uuid,
}

#[derive(Serialize)]
pub struct RenderedLine {
    /// The line's label
    pub label: String,
    /// Two ends of the line
    pub points: (Complex, Complex),
    /// Expression defining the line
    pub expr: Arc<Expression<LineExpr>>,
}

#[derive(Serialize)]
pub struct RenderedAngle {
    /// The angle's label
    pub label: String,
    /// Points defining the angle
    pub points: (Complex, Complex, Complex),
    /// Number of arcs in the angle
    pub no_arcs: u8,
    /// Expression that the angle was defined by
    pub expr: Arc<Expression<ScalarExpr>>,
    // Value of the angle (who'd have guessed)
    pub angle_value: f64,
}
#[derive(Serialize)]
pub struct RenderedSegment {
    /// Label of the segment
    pub label: String,
    /// Points defining the segment
    pub points: (Complex, Complex),
}

#[derive(Serialize)]
pub struct RenderedRay {
    /// Ray's label
    pub label: String,
    /// Points defining the ray
    pub points: (Complex, Complex),
    /// Second drawing point
    pub draw_point: Complex,
}

#[derive(Serialize)]
pub struct RenderedCircle {
    /// Circle's label
    pub label: String,
    /// Center of the circle
    pub center: Complex,
    /// Drawing point
    pub draw_point: Complex,
    /// Radius
    pub radius: f64,
}
/// Function getting the points defining the angle from the Expression defining it.
///
/// # Panics
/// It panics when the two lines that you are trying find crossing point of, are parallel.
fn get_angle_points(
    angle: &Arc<Expression<ScalarExpr>>,
    args: &EvaluationArgs,
) -> (Complex, Complex, Complex) {
    match &angle.kind {
        ScalarExpr::AnglePoint(AnglePoint { arm1, origin, arm2 }) => {
            let arm1 = arm1.evaluate(args).unwrap();
            let origin = origin.evaluate(args).unwrap();
            let arm2 = arm2.evaluate(args).unwrap();

            (arm1, origin, arm2)
        }
        ScalarExpr::AngleLine(AngleLine { k, l }) => {
            let ev_ln1 = k.evaluate(args).unwrap();
            let ev_ln2 = l.evaluate(args).unwrap();

            let origin = geometry::get_intersection(ev_ln1, ev_ln2).unwrap();

            (
                ev_ln1.origin + ev_ln1.direction,
                origin,
                ev_ln2.origin + ev_ln2.direction,
            )
        }
        _ => unreachable!(),
    }
}

/// Function getting the intersection points of the line with the picture's frame.
fn get_line_ends(figure: &Figure, ln_c: Line) -> (Complex, Complex) {
    fn choose_intersection(
        i: usize,
        j: usize,
    ) -> impl Fn(f64, &[Result<Complex, EvaluationError>]) -> &Complex {
        move |width, intersections| {
            intersections[i].as_ref().map_or_else(
                |_| intersections[j].as_ref().unwrap(),
                |x| {
                    if (x.real > 0f64 && x.real < width) || intersections[j].is_err() {
                        x
                    } else {
                        intersections[j].as_ref().unwrap()
                    }
                },
            )
        }
    }

    // +--0--+
    // |     |
    // 1     2
    // |     |
    // +--3--+

    #[allow(clippy::cast_precision_loss)]
    let width = figure.canvas_size.0 as f64;
    #[allow(clippy::cast_precision_loss)]
    let height = figure.canvas_size.1 as f64;

    let intersections = [
        geometry::get_intersection(
            ln_c,
            geometry::get_line(Complex::new(0.0, height), Complex::new(1.0, height)),
        ),
        geometry::get_intersection(
            ln_c,
            geometry::get_line(Complex::new(0.0, 0.0), Complex::new(0.0, 1.0)),
        ),
        geometry::get_intersection(
            ln_c,
            geometry::get_line(Complex::new(width, 0.0), Complex::new(width, 1.0)),
        ),
        geometry::get_intersection(
            ln_c,
            geometry::get_line(Complex::new(0.0, 0.0), Complex::new(1.0, 0.0)),
        ),
    ];

    // If the product of the real and imaginary is negative, line is "going down".
    let a = ln_c.direction.imaginary * ln_c.direction.real;

    #[allow(clippy::cast_precision_loss)]
    if a < 0f64 {
        // There must be one intersection with lines 0/1 and 2/3
        let i1 = choose_intersection(0, 1)(width, &intersections);

        let i2 = choose_intersection(0, 1)(width, &intersections);

        (*i1, *i2)
    } else {
        // There must be one intersection with lines 1/3 and 0/2
        let i1 = choose_intersection(3, 1)(width, &intersections);

        let i2 = choose_intersection(0, 1)(width, &intersections);

        (*i1, *i2)
    }
}

/// Pure utitlity function, used for scaling and transforming points which were missed by fn `project`().
fn transform(offset: Complex, scale: f64, size: Complex, pt: Complex) -> Complex {
    (pt - offset) * scale + size
}

/// Function that outputs the vector contaning the lines.
/// ///
/// # Panics
/// It shouldn't panic.
fn lines(
    figure: &Figure,
    offset: Complex,
    scale: f64,
    size: Complex,
    args: &EvaluationArgs,
) -> Vec<RenderedLine> {
    let mut blueprint_lines = Vec::new();
    for ln in &figure.lines {
        let mut ln_c = ln.evaluate(args).unwrap();
        ln_c.origin = transform(offset, scale, size, ln_c.origin);
        let line_ends = get_line_ends(figure, ln_c);
        blueprint_lines.push(RenderedLine {
            label: String::new(),
            points: (line_ends.0, line_ends.1),
            expr: Arc::clone(ln),
        });
    }
    blueprint_lines
}

/// Function that outputs the vector containing the angles.
///
/// # Panics
/// It shouldn't panic.
fn angles(
    figure: &Figure,
    offset: Complex,
    scale: f64,
    size: Complex,
    args: &EvaluationArgs,
) -> Vec<RenderedAngle> {
    let mut blueprint_angles = Vec::new();
    for ang in &figure.angles {
        let angle_points = get_angle_points(&ang.0, args);
        blueprint_angles.push(RenderedAngle {
            label: String::new(),
            points: (
                transform(offset, scale, size, angle_points.0),
                transform(offset, scale, size, angle_points.1),
                transform(offset, scale, size, angle_points.2),
            ),
            no_arcs: ang.1,
            expr: Arc::clone(&ang.0),
            angle_value: ang.0.evaluate(args).unwrap(),
        });
    }
    blueprint_angles
}

/// Function that outputs the vector contaning the segments.
///
/// # Panics
/// It shouldn't panic.
fn segments(
    figure: &Figure,
    offset: Complex,
    scale: f64,
    size: Complex,
    args: &EvaluationArgs,
) -> Vec<RenderedSegment> {
    let mut blueprint_segments = Vec::new();
    for segment in &figure.segments {
        let seg1 = segment.0.evaluate(args).unwrap();
        let seg2 = segment.1.evaluate(args).unwrap();
        blueprint_segments.push(RenderedSegment {
            label: String::new(),
            points: (
                transform(offset, scale, size, seg1),
                transform(offset, scale, size, seg2),
            ),
        });
    }
    blueprint_segments
}

fn rays(
    figure: &Figure,
    offset: Complex,
    scale: f64,
    size: Complex,
    args: &EvaluationArgs,
) -> Vec<RenderedRay> {
    let mut blueprint_rays = Vec::new();
    for ray in &figure.rays {
        let ray_a = ray.0.evaluate(args).unwrap();
        let ray_b = ray.1.evaluate(args).unwrap();

        let ray_a = transform(offset, scale, size, ray_a);
        let ray_b = transform(offset, scale, size, ray_b);

        let line = get_line(ray_a, ray_b);
        let intercepts = get_line_ends(figure, line);

        let vec1 = (ray_b - ray_a).normalize();
        let vec2 = (intercepts.1 - ray_a).normalize();
        let second_point;

        if vec1.real < 0.5 && vec1.real > -0.5 {
            if (vec1.imaginary - vec2.imaginary).abs() < 1e-4 {
                second_point = intercepts.1;
            } else {
                second_point = intercepts.0;
            }
        } else if (vec1.real - vec2.real).abs() < 1e-4 {
            second_point = intercepts.1;
        } else {
            second_point = intercepts.0;
        }

        blueprint_rays.push(RenderedRay {
            label: String::new(),
            points: (ray_a, second_point),
            draw_point: ray_b,
        });
    }

    blueprint_rays
}

fn circles(
    figure: &Figure,
    offset: Complex,
    scale: f64,
    size: Complex,
    args: &EvaluationArgs,
) -> Vec<RenderedCircle> {
    let mut blueprint_circles = Vec::new();
    for circle in &figure.circles {
        let circle = circle.evaluate(args).unwrap();
        let center = transform(offset, scale, size, circle.center);
        let draw_point = Complex::new(circle.center.real + circle.radius, circle.center.imaginary);
        let sc_rad = circle.radius * scale;
        blueprint_circles.push(RenderedCircle {
            label: String::new(),
            center,
            draw_point: transform(offset, scale, size, draw_point),
            radius: sc_rad,
        });
    }

    blueprint_circles
}

/// Takes the figure and rendered points and attempts to design a figure that can then be rendered in chosen format.
///
/// # Panics
/// Despite containing .unwrap() calls, it shouldn't panic.
///
/// # Errors
/// Returns an error if there is a problem with evaluating constructs (e. g. intersection of two parallel lines).
pub fn project(
    figure: &Figure,
    generated_points: &[(Adjustable, f64)],
    flags: &Arc<Flags>,
) -> Result<Output, EvaluationError> {
    let mut logger = Vec::new();
    let args = EvaluationArgs {
        logger: &mut logger,
        adjustables: generated_points,
        generation: 0,
        flags,
        cache: None,
    };

    let points: Vec<Complex> = figure
        .points
        .iter()
        .map(|pt| pt.0.evaluate(&args))
        .collect::<Result<Vec<Complex>, EvaluationError>>()?;

    #[allow(clippy::cast_precision_loss)]
    let size1 = Complex::new(figure.canvas_size.0 as f64, figure.canvas_size.1 as f64);
    let size09 = size1 * 0.9;
    let size005 = size1 * 0.05;

    // Frame top left point.
    let mut offset = points.get(0).copied().unwrap_or_default();

    //noinspection DuplicatedCode
    for x in &points {
        if x.real < offset.real {
            offset.real = x.real;
        }

        if x.imaginary < offset.imaginary {
            offset.imaginary = x.imaginary;
        }
    }

    // println!("Points pre-offset: {:?}", points);
    // println!("Offset: {offset}");
    let points: Vec<Complex> = points.into_iter().map(|x| x - offset).collect();
    // println!("Points post-offset: {:?}", points);

    // Frame bottom right point.
    let mut furthest = points.get(0).copied().unwrap_or_default();

    //noinspection DuplicatedCode
    for x in &points {
        if x.real > furthest.real {
            furthest.real = x.real;
        }

        if x.imaginary > furthest.imaginary {
            furthest.imaginary = x.imaginary;
        }
    }

    // The scaled frame should be at most (and equal for at least one dimension) 90% of the size of the desired image (margins for rendering).
    let scale = f64::min(
        size09.real / furthest.real,
        size09.imaginary / furthest.imaginary,
    );
    // println!("furthest: {furthest}, scale: {scale}");

    let points: Vec<Complex> = points.into_iter().map(|x| x * scale + size005).collect();

    let mut blueprint_points = Vec::new();

    for (i, pt) in points.iter().enumerate() {
        let pt_label =
            unroll::construct_point_name(figure.points[i].1.letter, figure.points[i].1.primes);
        let id = Uuid::new_v4();
        blueprint_points.push(Rc::new(RenderedPoint {
            label: pt_label,
            position: *pt,
            uuid: id,
        }));
    }

    // Creating a HashMap (the bridge between Expression defining the point and those points).
    let mut iden = HashMap::new();
    for (i, pt) in figure.points.clone().iter().enumerate() {
        let point = HashableArc::new(Arc::clone(&pt.0));
        iden.insert(point, Rc::clone(&blueprint_points[i]));
    }

    let blueprint_lines = lines(figure, offset, scale, size005, &args);

    let blueprint_angles = angles(figure, offset, scale, size005, &args);

    let blueprint_segments = segments(figure, offset, scale, size005, &args);

    let blueprint_rays = rays(figure, offset, scale, size005, &args);

    let blueprint_circles = circles(figure, offset, scale, size005, &args);

    Ok(Output {
        map: iden,
        vec_rendered: blueprint_points
            .into_iter()
            .map(Rendered::Point)
            .chain(blueprint_lines.into_iter().map(Rendered::Line))
            .chain(blueprint_angles.into_iter().map(Rendered::Angle))
            .chain(blueprint_segments.into_iter().map(Rendered::Segment))
            .chain(blueprint_rays.into_iter().map(Rendered::Ray))
            .chain(blueprint_circles.into_iter().map(Rendered::Circle))
            .collect(),
    })
}
