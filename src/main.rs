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
    ] {
        let ast = Ast::new("expression".to_string(), &variables, pattern);

        dbg!(pattern);
        dbg!(ast.root());

        eprintln!("########## <CODE> ##########");
        eprintln!("{}", ast.code());
        eprintln!("########## </CODE> ##########");
    }
}
