use hexpr::Operation;
use metacat::check::CheckResult;
use metacat::dv::{DvError, dv_check};
use metacat::theory::{Theory, TheoryId, TheorySet};
use std::collections::BTreeSet;

const SET: &str = include_str!("../../examples/mini-set.hex");

const FOUNDATIONS: &[&str] = &[
    "ax-mp", "ax-1", "ax-2", "ax-3", "ax-gen", "ax-4", "ax-5", "ax-6", "ax-7", "ax-8", "ax-9",
    "ax-10", "ax-11", "ax-12", "ax-13", "ax-ext", "ax-rep", "ax-sep", "ax-nul", "ax-pow", "ax-pr",
    "ax-un", "ax-reg", "ax-inf", "ax-inf2", "ax-cc", "ax-dc", "ax-ac", "ax-ac2",
];

fn theory_with(extra: &str) -> Theory {
    TheorySet::from_texts([SET, extra])
        .unwrap()
        .theories
        .remove(&TheoryId("set.proof".parse().unwrap()))
        .unwrap()
}

fn check_definition(theory: &Theory, name: &str) -> Result<CheckResult, DvError> {
    let arrow = theory.get_arrow(&name.parse().unwrap()).unwrap();
    let mut proof = arrow.definition.clone().expect("expected a proved theorem");
    dv_check(
        theory,
        arrow.type_maps.0.clone(),
        arrow.type_maps.1.clone(),
        arrow.ar.as_ref(),
        &mut proof,
    )
}

#[test]
fn foundational_axioms_are_present_without_assuming_theorems() {
    let theory = theory_with("");
    let Theory::Theory { arrows, .. } = &theory else {
        unreachable!();
    };
    let foundations: BTreeSet<_> = arrows
        .keys()
        .map(Operation::as_str)
        .filter(|name| name.starts_with("ax-"))
        .collect();
    assert_eq!(foundations, FOUNDATIONS.iter().copied().collect());

    let primitives: BTreeSet<_> = FOUNDATIONS
        .iter()
        .copied()
        .chain([
            "fresh",
            "wn",
            "wi",
            "wb",
            "wa",
            "wo",
            "wal",
            "cv",
            "wceq",
            "wex",
            "wcel",
            "wne",
            "wral",
            "wss",
            "c0",
            "wbr",
            "cdm",
            "crn",
            "csuc",
            "cfv",
            "com",
            "cen",
            "df-bi",
            "df-an",
            "df-or",
            "df-ex",
            "mm-copy-proof",
            "mm-discard-proof",
            "mm-copy-wff",
            "mm-discard-wff",
            "mm-copy-setvar",
            "mm-discard-setvar",
            "mm-copy-class",
            "mm-discard-class",
        ])
        .collect();
    assert_eq!(
        arrows
            .values()
            .filter(|arrow| arrow.definition.is_none())
            .map(|arrow| arrow.name.as_str())
            .collect::<BTreeSet<_>>(),
        primitives,
    );

    // Identity uses validate the annotations of unused axioms too, not just
    // the declarations reached by sp's proof.
    for name in primitives {
        let arrow = theory.get_arrow(&name.parse().unwrap()).unwrap();
        let mut proof = hexpr::try_interpret(&theory.local_signature(), &name.parse().unwrap())
            .unwrap()
            .map_nodes(|_| ());
        dv_check(
            &theory,
            arrow.type_maps.0.clone(),
            arrow.type_maps.1.clone(),
            arrow.ar.as_ref(),
            &mut proof,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn every_proof_checks_and_specialization_has_no_circular_dependencies() {
    let theory = theory_with("");
    let Theory::Theory { arrows, .. } = &theory else {
        unreachable!();
    };
    let definitions: BTreeSet<_> = arrows
        .values()
        .filter(|arrow| arrow.definition.is_some())
        .map(|arrow| arrow.name.clone())
        .collect();
    assert_eq!(definitions.len(), 146);
    for name in &definitions {
        check_definition(&theory, name.as_str()).unwrap_or_else(|error| panic!("{name}: {error}"));
    }

    fn visit(
        theory: &Theory,
        name: &Operation,
        active: &mut BTreeSet<Operation>,
        proved: &mut BTreeSet<Operation>,
    ) {
        if proved.contains(name) {
            return;
        }
        assert!(active.insert(name.clone()), "circular proof of {name}");
        if let Some(proof) = &theory.get_arrow(name).unwrap().definition {
            for dependency in &proof.hypergraph.edges {
                visit(theory, dependency, active, proved);
            }
        }
        active.remove(name);
        proved.insert(name.clone());
    }

    let mut proved = BTreeSet::new();
    visit(
        &theory,
        &"sp".parse().unwrap(),
        &mut BTreeSet::new(),
        &mut proved,
    );
    assert!(definitions.is_subset(&proved));
}

#[test]
fn specialization_allows_the_formula_to_contain_the_bound_variable() {
    let theory = theory_with(
        r#"
        (def set.proof sp-equality
          : setvar
          -> {[x.]
            ({([.x] cv) ([.x] cv)} = [eq.])
            ({([.x eq] forall) [.eq]} => |-)
          }
          = (mm-copy-setvar {_ mm-copy-setvar} {weq _} sp))
        "#,
    );
    check_definition(&theory, "sp-equality").unwrap();

    // Neither sp nor its use above needs x to be declared bare.
    let sp = theory.get_arrow(&"sp".parse().unwrap()).unwrap();
    assert!(sp.ar.is_none());
}

#[test]
fn ax_5_rejects_the_same_variable_occurring_in_its_formula() {
    let theory = theory_with(
        r#"
        (def set.proof bad-ax-5 (dv del)
          : setvar
          -> {[x.]
            ({([.x] cv) ([.x] cv)} = [eq.])
            ({[.eq] ([.x eq] forall)} => |-)
          }
          = (mm-copy-setvar {_ mm-copy-setvar} {weq _} ax-5))
        "#,
    );
    assert!(matches!(
        check_definition(&theory, "bad-ax-5"),
        Err(DvError::ForbiddenReachability { from: 0, to: 1, .. })
    ));
}

#[test]
fn explicit_proof_copying_preserves_the_formula() {
    let theory = theory_with(
        r#"
        (def set.proof copy-hypothesis
          : |- -> ([ph.ph ph] {|- |-}) = mm-copy-proof)
        (def set.proof discard-hypothesis : |- -> [ph.] = mm-discard-proof)
        (def set.proof bad-copy
          : ([ph ps.ph] |-) -> ([ph ps] {|- |-}) = mm-copy-proof)
        "#,
    );
    check_definition(&theory, "copy-hypothesis").unwrap();
    check_definition(&theory, "discard-hypothesis").unwrap();
    assert!(matches!(
        check_definition(&theory, "bad-copy"),
        Err(DvError::Check(
            metacat::check::Error::BoundaryCollision { .. }
        ))
    ));
}
