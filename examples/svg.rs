use open_hypergraphs::category::*;
use open_hypergraphs::lax::var::forget::forget_monogamous;

use metacat::dual::dual;
use metacat::fol::FOL;
use metacat::lang::{Obj, Term};
use metacat::svg::save_svg;
use metacat::util::build_typed;

// metamath alnex:  ⊢ (∀𝑥 ¬ 𝜑 ↔ ¬ ∃𝑥𝜑)

// df-ex source map: ⊢ ¬(∀x.φ)
fn alnex_source() -> Term<FOL> {
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
fn alnex_target() -> Term<FOL> {
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

fn main() {
    let alnex_src = dual(forget_monogamous(&alnex_source()));
    save_svg(&alnex_src, "alnex_source.svg").expect("Failed to save alnex_source SVG");

    let alnex_tgt = forget_monogamous(&alnex_target());
    save_svg(&alnex_tgt, "alnex_target.svg").expect("Failed to save alnex_target SVG");

    let both = alnex_src.compose(&alnex_tgt).unwrap();
    save_svg(&both, "composed.svg").expect("Failed to save composed SVG");
}
