use open_hypergraphs::category::*;
use open_hypergraphs::lax::{OpenHypergraph, var, var::forget::forget_monogamous};

use metacat::definition::Def;
use metacat::dual::dual;
use metacat::fol::{FOL, pretty_print_fol};
use metacat::interpreter::{Interpreter, Value};
use metacat::lang::{Obj, Term};
use metacat::proof;
use metacat::util::build_typed;

use std::collections::HashMap;
use std::fmt::{Debug, Display};

fn build_env() -> HashMap<proof::Path, proof::Type<FOL>> {
    HashMap::from([
        (
            "df-ex".to_string(),
            proof::Type {
                source: df_ex_source(),
                target: df_ex_target(),
            },
        ),
        (
            "con2bii".to_string(),
            proof::Type {
                source: con2bii_source(),
                target: con2bii_target(),
            },
        ),
    ])
}

fn build_proof() -> proof::Proof {
    build_typed([], |builder, []| {
        let df_ex = var::fn_operation(builder, &[], Obj, Def::Def("df-ex".to_string()));
        vec![var::fn_operation(
            builder,
            &[df_ex],
            Obj,
            Def::Def("con2bii".to_string()),
        )]
    })
    .unwrap()
}

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

fn save_svg<O: PartialEq + Clone + Display + Debug, A: PartialEq + Clone + Display + Debug>(
    term: &OpenHypergraph<O, A>,
    path: &str,
) {
    let _ = metacat::svg::save_svg(&term, path);
}

fn main() {
    let env = build_env()
        .into_iter()
        .map(|(k, v)| (k, v.composed()))
        .collect();
    let proof = forget_monogamous(&build_proof());

    // TODO: get strings for every node in the graph, plot it.
    let result = proof::check(proof.clone(), env).unwrap();
    println!("{:?}", result);
    print_interpreter_result("alnex", &result);

    let displayable = proof.map_edges(|e| format!("{:?}", e));
    //.with_nodes(|_| result)
    //.unwrap();
    println!("writing to alnex_proof.svg");
    save_svg(&displayable, "alnex_proof.svg");
}

fn print_interpreter_result(name: &str, values: &Vec<Value>) {
    for (i, value) in values.iter().enumerate() {
        println!("{}.{i}: {}", name, pretty_print_fol(value));
    }
}
