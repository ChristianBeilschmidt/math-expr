use evalexpr::{build_operator_tree, context_map};
use ocl::ProQue;
use rayon::prelude::*;
use serde::Serialize;
use wasmer::{imports, Instance, Module, Store, Value};

#[derive(Serialize)]
struct Result {
    n: usize,
    opencl: f64,
    evalexpr: f64,
    wasmer: f64,
}

fn main() {
    let mut csv = csv::Writer::from_writer(std::io::stdout());

    for mult in [1, 16, 32, 64] {
        let n: usize = 1_000_000 * mult;

        // println!("n = {}", n);

        let numbers_a = (0..n).map(|v| v as f64).collect::<Vec<_>>();
        let numbers_b = (0..n).map(|v| (n - v) as f64).collect::<Vec<_>>();

        let (opencl, opencl_result) = time_it(|| opencl(&numbers_a, &numbers_b));
        let (evalexpr, evalexpr_result) = time_it(|| evalexpr(&numbers_a, &numbers_b));
        let (wasmer, wasmer_result) = time_it(|| wasmer(&numbers_a, &numbers_b));

        csv.serialize(Result {
            n,
            opencl,
            evalexpr,
            wasmer,
        })
        .unwrap();

        // validate results
        assert_eq!(evalexpr_result, opencl_result);
        assert_eq!(evalexpr_result, wasmer_result);
    }
}

fn time_it(f: impl FnOnce() -> Vec<f64>) -> (f64, Vec<f64>) {
    let start = std::time::Instant::now();
    let result = f();
    let end = start.elapsed();
    let secs = end.as_secs() as f64 + end.subsec_nanos() as f64 / 1_000_000_000.0;

    // println!("{} took {} seconds", name, secs);

    (secs, result)
}

fn opencl(numbers_a: &[f64], numbers_b: &[f64]) -> Vec<f64> {
    assert_eq!(numbers_a.len(), numbers_b.len());

    let src = r#"
        __kernel void ndvi(__global double* out, __global const double* a, __global const double* b) {
            size_t i = get_global_id(0);
            out[i] = (a[i] - b[i]) / (a[i] + b[i]);
        }
    "#;

    let pro_que = ProQue::builder()
        .src(src)
        .dims(numbers_a.len())
        .build()
        .unwrap();

    let buffer = pro_que.create_buffer::<f64>().unwrap();
    let input_a = pro_que
        .buffer_builder::<f64>()
        .copy_host_slice(numbers_a)
        .build()
        .unwrap();
    let input_b = pro_que
        .buffer_builder::<f64>()
        .copy_host_slice(numbers_b)
        .build()
        .unwrap();

    let kernel = pro_que
        .kernel_builder("ndvi")
        .arg(&buffer)
        .arg(input_a)
        .arg(input_b)
        .build()
        .unwrap();

    unsafe {
        kernel.enq().unwrap();
    }

    let mut output = vec![0.0; numbers_a.len()];
    buffer.read(&mut output).enq().unwrap();

    output
}

fn evalexpr(numbers_a: &[f64], numbers_b: &[f64]) -> Vec<f64> {
    let expression = build_operator_tree("(a - b) / (a + b)").unwrap();

    numbers_a
        .par_iter()
        .zip_eq(numbers_b.par_iter())
        .map(|(&a, &b)| {
            expression
                .eval_float_with_context(
                    &context_map! {
                        "a" => a,
                        "b" => b,
                    }
                    .unwrap(),
                )
                .unwrap()
        })
        .collect()
}

fn wasmer(numbers_a: &[f64], numbers_b: &[f64]) -> Vec<f64> {
    let module_wat = r#"
    (module
        (func $ndvi
            (export "ndvi")
            (param $p0 f64) (param $p1 f64)
            (result f64)
            local.get $p0
            local.get $p1
            f64.sub
            local.get $p0
            local.get $p1
            f64.add
            f64.div
        )
    )
    "#;

    let store = Store::default();
    let module = Module::new(&store, &module_wat).unwrap();
    // The module doesn't import anything, so we create an empty import object.
    let import_object = imports! {};
    let instance = Instance::new(&module, &import_object).unwrap();

    let ndvi = instance.exports.get_function("ndvi").unwrap();

    numbers_a
        .par_iter()
        .zip_eq(numbers_b.par_iter())
        .map(|(&a, &b)| -> f64 {
            let result = ndvi.call(&[Value::F64(a), Value::F64(b)]).unwrap();

            result[0].unwrap_f64()
        })
        .collect()
}
