use open_hypergraphs::category::*;
use open_hypergraphs::lax::var::forget::forget_monogamous;

use metacat::dual::dual;
use metacat::fol::{FOL, pretty_print_fol};
use metacat::interpreter::{Interpreter, Value};
use metacat::lang::{Obj, Term};
use metacat::util::build_typed;

// alnex : ⊢ (∀𝑥 ¬ 𝜑 ↔ ¬ ∃𝑥𝜑)
fn alnex_target() -> Term<FOL> {
    use FOL::*;
    let result = build_typed([Obj, Obj], |builder, [a, x]| {
        // Left side: ∀𝑥 ¬ 𝜑
        let phi = Phi.call(builder, vec![a.clone(), x.clone()]);
        let not_phi = Not.call(builder, vec![phi]);
        let forall_not_phi = Forall.call(builder, vec![x.clone(), not_phi]);

        // Right side: ¬ ∃𝑥𝜑
        let phi2 = Phi.call(builder, vec![a, x.clone()]);
        let exists_phi = Exists.call(builder, vec![phi2, x]);
        let not_exists_phi = Not.call(builder, vec![exists_phi]);

        // ∀𝑥 ¬ 𝜑 ↔ ¬ ∃𝑥𝜑
        let equiv = Equiv.call(builder, vec![forall_not_phi, not_exists_phi]);
        let provable = Provable.call(builder, vec![equiv]);
        vec![provable]
    })
    .unwrap();
    forget_monogamous(&result)
}

fn alnex_source() -> Term<FOL> {
    // empty term - discards two metavars
    forget_monogamous(&build_typed([Obj, Obj], |_, [_, _]| vec![]).unwrap())
}

fn alnex() -> Term<FOL> {
    forget_monogamous(&dual(alnex_source()).compose(&alnex_target()).unwrap())
}

////////////////////////////////////////////////////////////////////////////////
// `df-ex : ⊢ (∃𝑥𝜑 ↔ ¬ ∀𝑥 ¬ 𝜑)` [https://us.metamath.org/mpeuni/def-ex.html]

// Take two metavariables (a, x), and
fn df_ex_source() -> Term<FOL> {
    // empty term - discards two metavars
    forget_monogamous(&build_typed([Obj, Obj], |_, [_, _]| vec![]).unwrap())
}

// construct ⊢ (∃𝑥𝜑 ↔ ¬ ∀𝑥 ¬ 𝜑)
fn df_ex_target() -> Term<FOL> {
    use FOL::*;
    build_typed([Obj, Obj], |builder, [a, x]| {
        // Left side: ∃𝑥𝜑
        let phi = Phi.call(builder, vec![a.clone(), x.clone()]);
        let exists_phi = Exists.call(builder, vec![phi, x.clone()]);

        // Right side: ¬ ∀𝑥 ¬ 𝜑
        let phi2 = Phi.call(builder, vec![a, x.clone()]);
        let not_phi = Not.call(builder, vec![phi2]);
        let forall_not_phi = Forall.call(builder, vec![x, not_phi]);
        let not_forall_not_phi = Not.call(builder, vec![forall_not_phi]);

        // ∃𝑥𝜑 ↔ ¬ ∀𝑥 ¬ 𝜑
        let equiv = Equiv.call(builder, vec![exists_phi, not_forall_not_phi]);
        let provable = Provable.call(builder, vec![equiv]);
        vec![provable]
    })
    .unwrap()
}

fn df_ex() -> Term<FOL> {
    forget_monogamous(&dual(df_ex_source()).compose(&df_ex_target()).unwrap())
}

////////////////////////////////////////////////////////////////////////////////
// `con2bii: (φ ↔ ¬ψ) ⇒ (ψ ↔ ¬ φ)` [https://us.metamath.org/mpeuni/con2bii.html]

fn con2bii_source() -> Term<FOL> {
    use FOL::*;
    let result = build_typed([Obj, Obj], |builder, [phi, psi]| {
        let not_psi = Not.call(builder, vec![psi]);
        let equiv = Equiv.call(builder, vec![phi, not_psi]);
        vec![Provable.call(builder, vec![equiv])]
    })
    .unwrap();
    forget_monogamous(&result)
}

fn con2bii_target() -> Term<FOL> {
    use FOL::*;
    let result = build_typed([Obj, Obj], |builder, [phi, psi]| {
        let not_phi = Not.call(builder, vec![phi]);
        let equiv = Equiv.call(builder, vec![psi, not_phi]);
        vec![Provable.call(builder, vec![equiv])]
    })
    .unwrap();
    forget_monogamous(&result)
}

fn con2bii() -> Term<FOL> {
    let s = con2bii_source();
    let t = con2bii_target();
    let d = dual(s);
    d.compose(&t).unwrap()
}

fn save_svg(term: &Term<FOL>, path: &str) {
    let _ = metacat::svg::save_svg(&term, path);
}

fn main() {
    let _ = save_svg(&df_ex(), "examples/images/df_ex.svg");

    // Run the interpreter on df_ex with no inputs
    let mut interpreter = Interpreter;
    let df_ex_tree = interpreter.run(df_ex(), vec![]).unwrap();
    print_interpreter_result("df_ex", &df_ex_tree);

    // now use df_ex_tree as input to con2bii...
    let term = con2bii();
    let _ = save_svg(&term, "examples/images/con2bii.svg");
    let con2bii_result = interpreter.run(term, df_ex_tree).unwrap();
    print_interpreter_result("con2bii(df_ex)", &con2bii_result);

    // Run alnex and compare to con2bii result
    let term = alnex();
    let _ = save_svg(&term, "examples/images/alnex.svg");
    let alnex_result = interpreter.run(term, vec![]).unwrap();
    print_interpreter_result("alnex", &alnex_result);

    if con2bii_result == alnex_result {
        println!("✓ alnex and con2bii results are equal!");
    } else {
        println!("✗ alnex and con2bii results differ");
    }
}

fn print_interpreter_result(name: &str, values: &Vec<Value>) {
    for (i, value) in values.iter().enumerate() {
        println!("{}.{i}: {}", name, pretty_print_fol(value));
    }
}
