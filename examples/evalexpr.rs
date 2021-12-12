/// * allows multiple data types
/// * easy to add UDFs
/// * variable assignments
fn main() {
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
