use std::sync::{Arc, Mutex};

/// * Only allows f64
/// * strange to combine UDFs with plain variables
#[allow(clippy::float_cmp)]
fn main() {
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
