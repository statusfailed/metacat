//! Soundness rejection tests. Every fixture is intended to be rejected, including
//! the known counterexamples: no ignores, should_panic, or inverted assertions.
//! An accepted fixture leaves this suite red until its underlying issue is fixed.
//! All fixtures must parse/load, so syntax errors do not count as soundness checks.
//!
//! Run: cargo test -p metacat --test counterexamples -- --nocapture
//! Most .hex fixtures load alongside counterexamples/base.hex. minimal_bottom.hex
//! stands alone; mini_set_contradiction.hex loads alongside examples/mini-set.hex.

use metacat::check::check;
use metacat::dv::{DvError, dv_check};
use metacat::theory::{Theory, TheoryId, TheorySet};

const BASE: &str = include_str!("../counterexamples/base.hex");

fn check_definition(theory: &Theory, name: &str) -> Result<(), DvError> {
    let declaration = theory.get_arrow(&name.parse().unwrap()).unwrap();
    let mut proof = declaration.definition.clone().expect("expected definition");
    dv_check(
        theory,
        declaration.type_maps.0.clone(),
        declaration.type_maps.1.clone(),
        declaration.ar.as_ref(),
        &mut proof,
    )
    .map(|_| ())
}

fn rejects(base: &str, name: &str, fixture: &str, theory_name: &str) {
    let theories = TheorySet::from_texts([base, fixture])
        .unwrap_or_else(|error| panic!("{name}: fixture must parse and load: {error}"));
    let theory = &theories.theories[&TheoryId(theory_name.parse().unwrap())];

    // Definition-replacement attacks must start with genuinely checked support
    // lemmas, not arbitrary new axioms or declarations whose bodies already fail.
    if let Theory::Theory { arrows, .. } = theory {
        for arrow in arrows.values().filter(|arrow| {
            arrow.definition.is_some()
                && matches!(arrow.name.as_str(), "lemma" | "wrapper" | "wrapper-again")
        }) {
            check_definition(theory, arrow.name.as_str()).unwrap_or_else(|error| {
                panic!("{name}: support lemma {} failed: {error}", arrow.name)
            });
        }
    }

    if name.starts_with("dv_") {
        let declaration = theory.get_arrow(&"bad".parse().unwrap()).unwrap();
        let mut proof = declaration.definition.clone().unwrap();
        check(
            theory,
            declaration.type_maps.0.clone(),
            declaration.type_maps.1.clone(),
            &mut proof,
        )
        .unwrap_or_else(|error| panic!("{name}: fixture must reach DV checking: {error}"));
    }

    let result = check_definition(theory, "bad");
    assert!(result.is_err(), "SOUNDNESS COUNTEREXAMPLE ACCEPTED: {name}");
    eprintln!("{name}: rejected: {}", result.unwrap_err());
}

macro_rules! counterexamples {
    ($($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                rejects(
                    BASE,
                    stringify!($name),
                    include_str!(concat!("../counterexamples/", stringify!($name), ".hex")),
                    "proof",
                );
            }
        )*
    };
}

counterexamples!(
    create_proof,
    create_open_proof,
    disconnected_conclusion,
    cyclic_proof,
    self_reference,
    mutual_reference,
    constructor_clash,
    boundary_collision,
    headed_boundary,
    occurs_cycle,
    nested_occurs_cycle,
    constructor_argument_alias,
    output_port_swap,
    merge_hypotheses,
    wellformed_is_provable,
    dv_capture,
    dv_transitive_capture,
    dv_missing_bareness,
    dv_weakened_permissions,
    dv_headed_variable,
    dv_wrapped_capture,
    opaque_constructor_clash,
);

#[test]
fn minimal_bottom() {
    rejects(
        "",
        "minimal_bottom",
        include_str!("../counterexamples/minimal_bottom.hex"),
        "proof",
    );
}

#[test]
fn mini_set_contradiction() {
    rejects(
        include_str!("../../examples/mini-set.hex"),
        "mini_set_contradiction",
        include_str!("../counterexamples/mini_set_contradiction.hex"),
        "set.proof",
    );
}

#[test]
fn positive_controls() {
    let theories = TheorySet::from_texts([
        BASE,
        r#"
        (def proof truth : {} -> (true |-) = true-intro)
        (def proof identity : |- -> |- = _)
        (def proof independent-instances : {|- |-} -> {|- |-} = {id id})
        (theory proof syntax {
          (arr discard-proof : |- -> [ph.])
        })
        (def proof discard : ([ph ps] {|- |-}) -> ([ph ps.ph] |-) = {_ discard-proof})
        (def proof safe-ax-5 (dv (del sel))
          : ([x ph.ph] wff)
          -> ([x ph.] { [.ph] ([.x ph] forall) } => |-)
          = ax-5)
        (def proof ports : (split {wff wff}) -> (split {wff wff}) = [a b.a b])
        "#,
    ])
    .unwrap();
    let theory = &theories.theories[&TheoryId("proof".parse().unwrap())];
    for name in [
        "truth",
        "identity",
        "independent-instances",
        "discard",
        "safe-ax-5",
        "ports",
    ] {
        check_definition(theory, name).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}
