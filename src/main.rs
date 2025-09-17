use open_hypergraphs::category::*;

use metacat::dual::dual;
use metacat::fol::FOL;
use metacat::forget::forget_monogamous;
use metacat::lang::{Obj, Term};
use metacat::svg::save_svg;
use metacat::util::build_typed;

// metamath's df-ex: ⊢ (∃𝑥𝜑 ↔ ¬ ∀𝑥 ¬ 𝜑)

// df-ex source map: ⊢ ¬(∀x.φ)
fn df_ex_target() -> Term<FOL> {
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
fn df_ex_source() -> Term<FOL> {
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
    let df_ex_tgt = forget_monogamous(&df_ex_target());
    save_svg(&df_ex_tgt, "df_ex_target.svg").expect("Failed to save df_ex_target SVG");

    let df_ex_src = dual(forget_monogamous(&df_ex_source()));
    save_svg(&df_ex_src, "df_ex_source.svg").expect("Failed to save df_ex_source SVG");

    let both = df_ex_src.compose(&df_ex_tgt).unwrap();
    save_svg(&both, "composed.svg").expect("Failed to save composed SVG");
}
