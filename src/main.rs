use math_expr::Ast;

fn main() {
    for (pattern, variables) in [
        ("1", vec![]),
        ("1 + 41", vec![]),
        ("1 + 2 / 3", vec![]),
        ("2**4", vec![]),
        ("a + 1", vec!["a".to_string()]),
        ("(a-b) / (a+b)", vec!["a".to_string(), "b".to_string()]),
        ("max(a, 0)", vec!["a".to_string()]),
        ("if true { 1 } else { 2 }", vec![]),
        ("if TRUE { 1 } else if false { 2 } else { 1 + 2 }", vec![]),
        (
            "if 1 < 2 { 1 } else if 1 + 5 < 3 - 1 { 2 } else { 1 + 2 }",
            vec![],
        ),
        (
            "if true && false {
                1
            } else if (1 < 2) && true {
                2
            } else {
                max(1, 2)
            }",
            vec![],
        ),
    ] {
        let ast = Ast::new("expression".to_string(), &variables, pattern);

        dbg!(pattern);
        dbg!(ast.root());

        eprintln!("########## <CODE> ##########");
        eprintln!("{}", ast.code());
        eprintln!("########## </CODE> ##########");
    }
}
