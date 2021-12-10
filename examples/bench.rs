use std::{fs::File, io::Write, process::Command};

use evalexpr::{build_operator_tree, context_map};
use libloading::{Library, Symbol};
use ocl::ProQue;
use rayon::prelude::*;
use serde::{Serialize, Serializer};
use wasmer::{imports, Instance, Module, Store, Value};

#[derive(Serialize)]
struct OutputRow {
    n: usize,
    #[serde(serialize_with = "serialize_f64")]
    opencl: f64,
    #[serde(serialize_with = "serialize_f64")]
    evalexpr: f64,
    #[serde(serialize_with = "serialize_f64")]
    wasmer: f64,
    #[serde(serialize_with = "serialize_f64")]
    dylib: f64,
    #[serde(serialize_with = "serialize_f64")]
    native: f64,
}

fn serialize_f64<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    const DECIMAL_SEPARATOR: char = ',';

    let value_string = value.to_string();
    let value_split: Vec<_> = value_string.split('.').collect();

    match value_split.len() {
        1 => serializer.serialize_str(value_split[0]),
        2 => serializer.serialize_str(&format!(
            "{}{}{}",
            value_split[0], DECIMAL_SEPARATOR, value_split[1]
        )),
        _ => panic!("this is a weird number: {:?}", value_split),
    }
}

fn main() {
    let mut csv = csv::WriterBuilder::new()
        .delimiter(b';')
        .has_headers(true)
        .from_writer(std::io::stdout());

    for mult in [1, 16, 32, 64] {
        let n: usize = 1_000_000 * mult;

        // println!("n = {}", n);

        let numbers_a = (0..n).map(|v| v as f64).collect::<Vec<_>>();
        let numbers_b = (0..n).map(|v| (n - v) as f64).collect::<Vec<_>>();

        let (opencl, opencl_result) = time_it(|| opencl(&numbers_a, &numbers_b));
        let (evalexpr, evalexpr_result) = time_it(|| evalexpr(&numbers_a, &numbers_b));
        let (wasmer, wasmer_result) = time_it(|| wasmer(&numbers_a, &numbers_b));
        let (dylib, dylib_result) = time_it(|| dylib(&numbers_a, &numbers_b));
        let (native, native_result) = time_it(|| native(&numbers_a, &numbers_b));

        csv.serialize(OutputRow {
            n,
            opencl,
            evalexpr,
            wasmer,
            dylib,
            native,
        })
        .unwrap();

        // validate results
        assert_eq!(evalexpr_result, opencl_result);
        assert_eq!(evalexpr_result, wasmer_result);
        assert_eq!(evalexpr_result, dylib_result);
        assert_eq!(evalexpr_result, native_result);
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

fn dylib(numbers_a: &[f64], numbers_b: &[f64]) -> Vec<f64> {
    type BiFunc = unsafe fn(f64, f64) -> f64;

    let input_filename = "tmp/ndvi.rs";
    let library_filename = "tmp/libndvi.so";

    let mut file = File::create(input_filename).unwrap();
    file.write_all(
        br#"
        #[no_mangle]
        pub extern "C" fn ndvi(a: f64, b: f64) -> f64 {
            (a - b) / (a + b)
        }
    "#,
    )
    .unwrap();

    let mut compile_file = Command::new("rustc");
    compile_file
        .args(&["--crate-type", "cdylib", "--out-dir", "tmp", input_filename])
        .status()
        .expect("process failed to execute");

    let lib = unsafe { Library::new(library_filename) }.unwrap();
    let ndvi = unsafe {
        let func: Symbol<BiFunc> = lib.get(b"ndvi").unwrap();
        func
    };

    numbers_a
        .par_iter()
        .zip_eq(numbers_b.par_iter())
        .map(|(&a, &b)| unsafe { ndvi(a, b) })
        .collect()
}

fn native(numbers_a: &[f64], numbers_b: &[f64]) -> Vec<f64> {
    numbers_a
        .par_iter()
        .zip_eq(numbers_b.par_iter())
        .map(|(&a, &b)| (a - b) / (a + b))
        .collect()
}
