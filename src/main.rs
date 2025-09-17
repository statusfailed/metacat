use open_hypergraphs::category::*;

use metacat::dual::dual;
use metacat::fol::FOL;
use metacat::forget::forget_monogamous;
use metacat::lang::{Obj, Term};
use metacat::svg::save_svg;
use metacat::util::build_typed;

// ⊢ ¬(∀x.φ)
fn qneg_source() -> Term<FOL> {
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
fn qneg_target() -> Term<FOL> {
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
    let qn_src = dual(forget_monogamous(&qneg_source()));
    save_svg(&qn_src, "qneg_source.svg").expect("Failed to save qneg_source SVG");

    let qn_tgt = &forget_monogamous(&qneg_target());
    save_svg(&qn_tgt, "qneg_target.svg").expect("Failed to save qneg_target SVG");

    let both = qn_src.compose(qn_tgt).unwrap();
    save_svg(&both, "composed.svg").expect("Failed to save composed SVG");
}
