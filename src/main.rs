use std::sync::{Arc, Mutex};

mod wasmer;

fn main() {
    wasmer::test_wasmer();

    exmex();
    evalexpr();
    test_fasteval();
}

/// * Only allows f64
/// * strange to combine UDFs with plain variables
#[allow(clippy::float_cmp)]
fn test_fasteval() {
    use fasteval::Compiler;
    use fasteval::Evaler; // use this trait so we can call eval().
    use std::collections::BTreeMap; // use this trait so we can call compile().

    let parser = fasteval::Parser::new();
    let mut slab = fasteval::Slab::new();

    (|| -> Result<_, fasteval::Error> {
        let expr_str = "min(a, b)";
        let compiled = parser
            .parse(expr_str, &mut slab.ps)?
            .from(&slab.ps)
            .compile(&slab.ps, &mut slab.cs);

        let mut map = BTreeMap::new();
        map.insert("a".to_string(), 2.);
        map.insert("b".to_string(), 3.);
        // When working with compiled constant expressions, you can use the
        // eval_compiled*!() macros to save a function call:
        let val = fasteval::eval_compiled!(compiled, &slab, &mut map);

        assert_eq!(val, 2.);

        Ok(())
    })()
    .unwrap();

    (|| -> Result<_, fasteval::Error> {
        let variables = Arc::new(Mutex::new(BTreeMap::new()));
        variables.lock().unwrap().insert("a".to_string(), 2.);
        variables.lock().unwrap().insert("b".to_string(), 3.);

        let mut ns = |name: &str, args: Vec<f64>| -> Option<f64> {
            if let Some(value) = variables.lock().unwrap().get(name) {
                return Some(*value);
            }

            match name {
                "ndvi" if args.len() == 2 => {
                    let a = args[0];
                    let b = args[1];
                    let ndvi: f64 = (a - b) / (a + b);
                    Some(ndvi)
                }
                _ => None,
            }
        };

        let expr_str = "ndvi(a, b)";
        let compiled = parser
            .parse(expr_str, &mut slab.ps)?
            .from(&slab.ps)
            .compile(&slab.ps, &mut slab.cs);

        // When working with compiled constant expressions, you can use the
        // eval_compiled*!() macros to save a function call:
        let val = fasteval::eval_compiled!(compiled, &slab, &mut ns);

        assert_eq!(val, -0.2);

        variables.lock().unwrap().insert("a".to_string(), 0.);

        let val = fasteval::eval_compiled!(compiled, &slab, &mut ns);

        assert_eq!(val, -1.0);

        Ok(())
    })()
    .unwrap();
}

/// * allows multiple data types
/// * easy to add UDFs
/// * variable assignments
fn evalexpr() {
    use evalexpr::*;

    // simple expression
    {
        let mut context = context_map! {
            "a" => 2,
            "b" => 3
        }
        .unwrap();

        let expression = build_operator_tree("min(a, b)").unwrap();

        assert_eq!(expression.eval_int_with_context_mut(&mut context), Ok(2));
    }

    // variables
    {
        let mut context = context_map! {
            "a" => 2,
            "b" => 3
        }
        .unwrap();

        let expression = build_operator_tree("foo = a + b; foo").unwrap();

        assert_eq!(expression.eval_int_with_context_mut(&mut context), Ok(5));
    }

    // udf
    {
        let context = context_map! {
            "a" => 2,
            "b" => 3,
            "ndvi" => Function::new(|args| {
                let args = args.as_fixed_len_tuple(2)?;
                let a = args[0].as_float().or_else(|_| args[0].as_int().map(|v| v as f64))?;
                let b = args[1].as_float().or_else(|_| args[1].as_int().map(|v| v as f64))?;
                let ndvi: f64 = (a - b) / (a + b);
                Ok(Value::Float(ndvi))
            }),
        }
        .unwrap();

        let expression = build_operator_tree("ndvi(a, b)").unwrap();

        assert_eq!(expression.eval_float_with_context(&context), Ok(-0.2));
    }
}

/// * works for multiple data types
/// * cannot add functions with more than one variable that is not an operator
/// * differentiation between variables and UDFs
fn exmex() {
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
