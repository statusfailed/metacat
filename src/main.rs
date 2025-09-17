use metacat::fol::FOL;
use metacat::lang::Term;
use metacat::util::build_typed;

fn qneg_source() -> Term<FOL> {
    use FOL::*;
    build_typed([(), ()], |builder, [x, a]| {
        let y = Phi.call(builder, vec![a, x.clone()]);
        let y = Forall.call(builder, vec![y, x]);
        let y = Not.call(builder, vec![y]);
        let y = Provable.call(builder, vec![y]);
        vec![y]
    })
    .unwrap()
}

fn main() {
    // TODO: render SVG of qneg_source write to qneg_source.svg.
}
