use metacat::check::{CheckResult, check};
use metacat::dv::{DvError, DvLocation, dv_check};
use metacat::theory::{Theory, TheoryId, TheorySet};
use open_hypergraphs::category::Arrow;
use open_hypergraphs::lax::EdgeId;

const THEORY: &str = r#"
(theory syntax nat {
  (arr f : 1 -> 1)
  (arr zero : 0 -> 1)
})
(theory proof syntax {
  (arr guard (dv (del sel)) : [x ph] -> [x ph])
  (arr dependent (dv _) : [x ph] -> [x ph])
  (arr distinct (dv {del del}) : [x y] -> [x y])
  (arr selective (dv {_ sel}) : [x ph ps] -> [x ph ps])
  (arr plain : [x ph] -> [x ph])
  (arr malformed (dv dup) : [x ph] -> [x ph])
})
"#;

// Every fixture here is valid for the ordinary checker, so negative cases
// specifically test the additional DV conditions.
fn check_definition(theory: &Theory, name: &str) -> Result<CheckResult, DvError> {
    let declaration = theory.get_arrow(&name.parse().unwrap()).unwrap();
    let mut definition = declaration.definition.clone().unwrap();
    let (source, target) = declaration.type_maps.clone();
    check(theory, source.clone(), target.clone(), &mut definition)
        .expect("fixture must pass ordinary checking");
    dv_check(
        theory,
        source,
        target,
        declaration.ar.as_ref(),
        &mut definition,
    )
}

fn check_example(definition: &str) -> Result<CheckResult, DvError> {
    let theories = TheorySet::from_texts([THEORY, definition]).unwrap();
    let theory = &theories.theories[&TheoryId("proof".parse().unwrap())];
    check_definition(theory, "example")
}

#[test]
fn bad_ax_5_requires_the_missing_boundary_declaration() {
    let theories = TheorySet::from_text(include_str!("../../examples/fol.hex")).unwrap();
    let theory = &theories.theories[&TheoryId("fol.proof".parse().unwrap())];
    assert!(matches!(
        check_definition(theory, "bad-ax-5"),
        Err(DvError::BoundaryBareness { boundary: 0, .. })
    ));
    check_definition(theory, "win").unwrap();

    // Restoring ax-5's condition is sufficient to validate the same proof.
    let declaration = theory.get_arrow(&"bad-ax-5".parse().unwrap()).unwrap();
    let axiom = theory.get_arrow(&"ax-5".parse().unwrap()).unwrap();
    let mut definition = declaration.definition.clone().unwrap();
    dv_check(
        theory,
        declaration.type_maps.0.clone(),
        declaration.type_maps.1.clone(),
        axiom.ar.as_ref(),
        &mut definition,
    )
    .unwrap();
}

#[test]
fn boundary_permissions_cannot_weaken_an_internal_condition() {
    check_example("(def proof example (dv (del sel)) : [x ph] -> [x ph] = guard)").unwrap();
    assert!(matches!(
        check_example("(def proof example (dv _) : [x ph] -> [x ph] = guard)"),
        Err(DvError::ForbiddenReachability { from: 0, to: 1, .. })
    ));
}

#[test]
fn boundary_extension_propagates_through_constructor_paths() {
    check_example(
        r#"
        (def proof example (dv (del sel))
          : ([x ph . x ph] {_ (f f)}) -> ([x ph . x ph] {_ (f f)}) = guard)
    "#,
    )
    .unwrap();
    assert!(matches!(
        check_example(
            r#"
            (def proof example (dv _)
              : ([x ph . x ph] {_ (f f)}) -> ([x ph . x ph] {_ (f f)}) = guard)
        "#
        ),
        Err(DvError::ForbiddenReachability { from: 0, to: 1, .. })
    ));
}

#[test]
fn rejects_actual_constructor_dependencies() {
    assert!(matches!(
        check_example(
            r#"
            (def proof example (dv del)
              : ([x . x x] {_ (f f)}) -> ([x . x x] {_ (f f)}) = guard)
        "#
        ),
        Err(DvError::ForbiddenReachability { from: 0, to: 1, .. })
    ));
}

#[test]
fn explicitly_permitted_dependencies_are_accepted() {
    check_example("(def proof example (dv _) : [x ph] -> [x ph] = dependent)").unwrap();
    check_example(
        r#"
        (def proof example (dv del)
          : ([x . x x] {_ (f f)}) -> ([x . x x] {_ (f f)}) = dependent)
    "#,
    )
    .unwrap();
}

#[test]
fn bare_metavariables_cannot_be_substituted_with_constants() {
    assert!(matches!(
        check_example("(def proof example : {zero _} -> {zero _} = guard)"),
        Err(DvError::InternalBareness {
            metavariable: 0,
            ..
        })
    ));
}

#[test]
fn reflexive_reachability_detects_identified_metavariables() {
    // Both variables are bare, but they must remain distinct (the id_B block).
    assert!(matches!(
        check_example("(def proof example (dv del) : [x . x x] -> [x . x x] = distinct)"),
        Err(DvError::ForbiddenReachability { from: 0, to: 1, .. })
    ));
    // B→C is permitted here, but identifying B with C violates the C→B zero block.
    assert!(matches!(
        check_example("(def proof example (dv del) : [x . x x] -> [x . x x] = dependent)"),
        Err(DvError::ForbiddenReachability { from: 1, to: 0, .. })
    ));
    // Formula identifications are allowed by the all-one C→C block.
    check_example("(def proof example : [x . x x] -> [x . x x] = plain)").unwrap();
}

#[test]
fn permissions_are_not_transitively_closed_or_merged_by_union() {
    // selective permits x→ph but forbids x→ps. The all-one C→C block
    // must not make x→ps permissible, even when ph and ps are identified.
    assert!(matches!(
        check_example(
            r#"
            (def proof example (dv _) : [x ph . x ph ph] -> [x ph . x ph ph] = selective)
        "#
        ),
        Err(DvError::ForbiddenReachability { from: 0, to: 2, .. })
    ));
}

#[test]
fn transports_each_occurrences_metavariables_to_phi() {
    let result = check_example(
        r#"
        (def proof example (dv {del del sel sel})
          : [x y ph ps . x ph y ps] -> [x y ph ps . x ph y ps] = {guard guard})
    "#,
    )
    .unwrap();
    let boundary: Vec<_> = result
        .phi
        .sources
        .iter()
        .map(|node| result.saturation.table.0[node.0])
        .collect();
    assert_eq!(result.generator_metavariables.len(), 2);
    for (i, witness) in result.generator_metavariables.iter().enumerate() {
        assert_eq!(witness.occurrence, EdgeId(i));
        assert_eq!(witness.mapping.target, result.phi.hypergraph.nodes.len());
        assert_eq!(
            witness.mapping.compose(&result.saturation).unwrap().table.0,
            vec![boundary[i], boundary[i + 2]]
        );
    }
    // Only the second occurrence sees the forbidden y→ps dependency.
    assert!(matches!(
        check_example(
            r#"
            (def proof example (dv {del sel _})
              : [x y ph ps . x ph y ps] -> [x y ph ps . x ph y ps] = {guard guard})
        "#
        ),
        Err(DvError::ForbiddenReachability {
            location: DvLocation::Generator {
                occurrence: EdgeId(1),
                ..
            },
            ..
        })
    ));
}

#[test]
fn absent_annotations_allow_formula_dependencies_and_extra_boundary_bareness() {
    check_example(
        r#"
        (def proof example : ([x . x x] {_ f}) -> ([x . x x] {_ f}) = plain)
    "#,
    )
    .unwrap();
    check_example("(def proof example (dv {del del}) : [x ph] -> [x ph] = plain)").unwrap();
    check_example("(def proof example (dv {sel sel}) : [x ph] -> [x ph] = plain)").unwrap();
}

#[test]
fn validates_annotation_dimensions_at_boundary_and_generator() {
    assert!(matches!(
        check_example("(def proof example (dv dup) : [x ph] -> [x ph] = plain)"),
        Err(DvError::InvalidArity {
            location: DvLocation::Boundary,
            ..
        })
    ));
    assert!(matches!(
        check_example("(def proof example : [x ph] -> [x ph] = malformed)"),
        Err(DvError::InvalidArity {
            location: DvLocation::Generator { .. },
            ..
        })
    ));
}

#[test]
fn ordinary_checking_errors_are_preserved() {
    let theories = TheorySet::from_texts([
        THEORY,
        "(def proof example : [x ph] -> ([x ph] {f _}) = plain)",
    ])
    .unwrap();
    let theory = &theories.theories[&TheoryId("proof".parse().unwrap())];
    let declaration = theory.get_arrow(&"example".parse().unwrap()).unwrap();
    let mut definition = declaration.definition.clone().unwrap();
    assert!(matches!(
        dv_check(
            theory,
            declaration.type_maps.0.clone(),
            declaration.type_maps.1.clone(),
            None,
            &mut definition,
        ),
        Err(DvError::Check(_))
    ));
}
