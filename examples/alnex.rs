use open_hypergraphs::category::*;

use metacat::dual::dual;
use metacat::fol::FOL;
use metacat::forget::forget_monogamous;
use metacat::lang::{Obj, Term};
use metacat::svg::save_svg;
use metacat::util::build_typed;

// metamath alnex:  ⊢ (∀𝑥 ¬ 𝜑 ↔ ¬ ∃𝑥𝜑)

// df-ex source map: ⊢ ¬(∀x.φ)
fn alnex_lhs() -> Term<FOL> {
    use FOL::*;
    build_typed([Obj, Obj], |builder, [a, x]| {
        let y = Phi.call(builder, vec![a, x.clone()]);
        let y = Forall.call(builder, vec![y, x]);
        let y = Not.call(builder, vec![y]);
        let y = Provable.call(builder, vec![y]);
        vec![y]
    })
    .unwrap()
}

// ⊢ (∃x.¬φ)
fn alnex_rhs() -> Term<FOL> {
    use FOL::*;
    build_typed([Obj, Obj], |builder, [a, x]| {
        let y = Phi.call(builder, vec![a, x.clone()]);
        let y = Not.call(builder, vec![y]);
        let y = Exists.call(builder, vec![y, x]);
        let y = Provable.call(builder, vec![y]);
        vec![y]
    })
    .unwrap()
}

// alnex() would put these together with a ↔ and turnstile. but don't worry about this.
//fn alnex() -> Term<FOL> {
//  todo!()
//}

////////////////////////////////////////////////////////////////////////////////
// `df-ex : ⊢ (∃𝑥𝜑 ↔ ¬ ∀𝑥 ¬ 𝜑)` [https://us.metamath.org/mpeuni/def-ex.html]

// Take two metavariables (a, x), and
fn df_ex_source() -> Term<FOL> {
    // empty term - discards two metavars
    forget_monogamous(&build_typed([Obj, Obj], |_, [_, _]| vec![]).unwrap())
}

//TODO: construct ⊢ (∃𝑥𝜑 ↔ ¬ ∀𝑥 ¬ 𝜑)
fn df_ex_target() -> Term<FOL> {
    todo!()
}

fn df_ex() -> Term<FOL> {
    todo!()
}

////////////////////////////////////////////////////////////////////////////////
// `con2bii: (φ ↔ ¬ψ) ⇒ (ψ ↔ ¬ φ)` [https://us.metamath.org/mpeuni/con2bii.html]

fn con2bii_source() -> Term<FOL> {
    use FOL::*;
    let result = build_typed([Obj, Obj], |builder, [phi, psi]| {
        let not_psi = Not.call(builder, vec![psi]);
        vec![Equiv.call(builder, vec![phi, not_psi])]
    })
    .unwrap();
    forget_monogamous(&result)
}

fn con2bii_target() -> Term<FOL> {
    use FOL::*;
    let result = build_typed([Obj, Obj], |builder, [phi, psi]| {
        let not_phi = Not.call(builder, vec![phi]);
        vec![Equiv.call(builder, vec![psi, not_phi])]
    })
    .unwrap();
    forget_monogamous(&result)
}

fn con2bii() -> Term<FOL> {
    dual(con2bii_source()).compose(&con2bii_target()).unwrap()
}

fn main() {
    todo!()
}
