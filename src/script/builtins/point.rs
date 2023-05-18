use crate::script::{
    token::{Position, Span},
    unroll::{
        CompileContext, Function
    }
};

use super::{overload, free};

pub fn register(context: &mut CompileContext) {
    context.functions.insert(
        String::from("Point"),
        Function {
            name: String::from("Point"),
            overloads: vec![
                overload!(() -> POINT {
                    |_, _, _| free!(POINT)
                })
            ],
        },
    );
}
