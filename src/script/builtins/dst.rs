/*
Copyright (c) 2023 Michał Wilczek, Michał Margos

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

#[allow(unused_imports)]
use crate::script::unroll::{Expr, Function, Library, Line, Point, Scalar};
use geo_aid_derive::overload;

#[allow(unused_imports)]
use super::macros::{distance, set_unit};

pub fn register(library: &mut Library) {
    library.functions.insert(
        String::from("dst"),
        Function {
            name: String::from("dst"),
            overloads: vec![
                overload!((DISTANCE) -> DISTANCE {
                    |v: Expr<Scalar>, _, _| v
                }),
                overload!((SCALAR) -> DISTANCE {
                    |mut v: Expr<Scalar>, _, _| set_unit!(v, %DISTANCE)
                }),
                overload!((POINT, POINT) -> DISTANCE {
                    |mut a: Expr<Point>, mut b: Expr<Point>, _, _| distance!(PP: a, b)
                }),
                overload!((POINT, LINE) -> DISTANCE {
                    |mut a: Expr<Point>, mut k: Expr<Line>, _, _| distance!(PL: a, k)
                }),
                overload!((LINE, POINT) -> DISTANCE {
                    |mut k: Expr<Line>, mut a: Expr<Point>, _, _| distance!(PL: a, k)
                }),
            ],
        },
    );
}
