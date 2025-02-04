// Copyright (c) The Diem Core Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::testutils::compile_module_string;

#[test]
fn compile_module_with_functions() {
    let code = String::from(
        "
        module 0x27.Foobar {
            struct FooCoin { value: u64 }

            public value(this: &Self.FooCoin): u64 {
                let value_ref: &u64;
            label b0:
                value_ref = &move(this).FooCoin::value;
                return *move(value_ref);
            }

            public deposit(this: &mut Self.FooCoin, check: Self.FooCoin) {
                let value_ref: &mut u64;
                let value: u64;
                let check_ref: &Self.FooCoin;
                let check_value: u64;
                let new_value: u64;
                let i: u64;
            label b0:
                value_ref = &mut move(this).FooCoin::value;
                value = *copy(value_ref);
                check_ref = &check;
                check_value = Self.value(move(check_ref));
                new_value = copy(value) + copy(check_value);
                *move(value_ref) = move(new_value);
                FooCoin { value: i } = move(check);
                return;
            }
        }
        ",
    );
    let compiled_module_res = compile_module_string(&code);
    assert!(compiled_module_res.is_ok());
}

fn generate_function(name: &str, num_formals: usize, num_locals: usize) -> String {
    let mut code = format!("public {}(", name);

    code.reserve(30 * (num_formals + num_locals));

    for i in 0..num_formals {
        code.push_str(&format!("formal_{}: u64", i));
        if i < num_formals - 1 {
            code.push_str(", ");
        }
    }

    code.push_str(") {\n");

    for i in 0..num_locals {
        code.push_str(&format!("let x_{}: u64;\n", i));
    }
    code.push_str("label b0:\n");
    for i in 0..num_locals {
        code.push_str(&format!("x_{} = {};\n", i, i));
    }

    code.push_str("return;");

    code.push('}');

    code
}

#[test]
fn compile_module_with_large_frame() {
    let mut code = String::from(
        "
        module 0x16.Foobar {
            struct FooCoin { value: u64 }
        ",
    );

    // Max number of locals (formals + local variables) is u8::max_value().
    code.push_str(&generate_function("foo_func", 128, 127));

    code.push('}');

    let compiled_module_res = compile_module_string(&code);
    assert!(compiled_module_res.is_ok());
}

#[test]
fn compile_module_with_script_visibility_functions() {
    let code = String::from(
        "
        module 0xa1.Foobar {
            public(script) foo() {
            label b0:
                return;
            }

            public(script) bar() {
            label b0:
                Self.foo();
                return;
            }
        }
        ",
    );
    let compiled_module_res = compile_module_string(&code);
    assert!(compiled_module_res.is_ok());
}
