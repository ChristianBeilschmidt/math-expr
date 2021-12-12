use wasmer::{imports, Function, Instance, Module, Store, Value};

pub fn main() {
    // let module_wat = r#"
    // (module
    // (type $t0 (func (param i32) (result i32)))
    // (func $add_one (export "add_one") (type $t0) (param $p0 i32) (result i32)
    //     get_local $p0
    //     i32.const 1
    //     i32.add))
    // "#;

    let module_wat = r#"
    (module
        (func $max (import "env" "max") (param i32 i32) (result i32))

        (type $t0 (func (param i32) (result i32)))
        (func $add_one (export "add_one") (type $t0) (param $p0 i32) (result i32)
            (call $max (local.get $p0) (i32.const 100))
            i32.const 1
            i32.add))
    "#;

    let store = Store::default();
    let module = Module::new(&store, &module_wat).unwrap();
    // The module doesn't import anything, so we create an empty import object.
    let import_object = imports! {
        "env" => {
            "max" => Function::new_native(&store, i32::max),
        },
    };
    let instance = Instance::new(&module, &import_object).unwrap();

    let add_one = instance.exports.get_function("add_one").unwrap();

    let result = add_one.call(&[Value::I32(42)]).unwrap();

    assert_eq!(result[0], Value::I32(101));

    let result = add_one.call(&[Value::I32(142)]).unwrap();

    assert_eq!(result[0], Value::I32(143));
}
