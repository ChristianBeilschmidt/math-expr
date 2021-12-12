/// * works for multiple data types
/// * cannot add functions with more than one variable that is not an operator
/// * differentiation between variables and UDFs
fn main() {
    use exmex::prelude::*;
    use exmex::{ops_factory, BinOp, Express, MakeOperators, Operator, Val};

    ops_factory!(
        BitwiseOpsFactory,
        u32,
        Operator::make_bin(
            "|",
            BinOp {
                apply: |a, b| a | b,
                prio: 0,
                is_commutative: true,
            }
        ),
        Operator::make_unary("!", |a| !a)
    );
    let expr = FlatEx::<_, BitwiseOpsFactory>::from_str("!(a|b)").unwrap();
    let result = expr.eval(&[0, 1]).unwrap();

    assert_eq!(result, u32::MAX - 1);

    let expr = exmex::parse_val::<i32, f64>("2 * a").unwrap();

    assert_eq!(expr.eval(&[Val::Int(1)]).unwrap(), Val::Int(2));
    assert_eq!(expr.eval(&[Val::Int(1)]).unwrap(), Val::Float(2.));

    // ops_factory!(
    //     IntegerOpsFactory, // name of the factory type
    //     i32,               // data type of the operands
    //     Operator::make_bin(
    //         "ndvi",
    //         BinOp {
    //             apply: |a, b| {
    //                 let a = a as f64;
    //                 let b = b as f64;
    //                 ((a - b) / (a + b)) as i32
    //             },
    //             prio: 1,
    //             is_commutative: false,
    //         }
    //     )
    // );

    // let expr = exmex::parse_val::<i32, f64>("ndvi(a, b)").unwrap();

    // assert_eq!(expr.eval(&[Val::Int(2), Val::Int(3)]).unwrap(), Val::Int(2));
}
