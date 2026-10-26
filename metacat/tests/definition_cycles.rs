use metacat::check::{Error, check};
use metacat::theory::{Theory, TheoryId, TheorySet};

const THEORY: &str = r#"
    (theory syntax nat {
      (arr wff : 1 -> 1)
    })
    (theory proof syntax {
      (arr id : wff -> wff)
    })
"#;

fn theory_with(definitions: &str) -> Theory {
    TheorySet::from_texts([THEORY, definitions])
        .unwrap()
        .theories
        .remove(&TheoryId("proof".parse().unwrap()))
        .unwrap()
}

fn check_definition(theory: &Theory, name: &str) -> Result<(), Error> {
    let declaration = theory.get_arrow(&name.parse().unwrap()).unwrap();
    check(
        theory,
        declaration.type_maps.0.clone(),
        declaration.type_maps.1.clone(),
        &mut declaration.definition.clone().unwrap(),
    )
    .map(|_| ())
}

#[test]
fn rejects_self_reference() {
    let theory = theory_with("(def proof bad : wff -> wff = bad)");
    assert!(matches!(
        check_definition(&theory, "bad"),
        Err(Error::CyclicDefinition { operation }) if operation.as_str() == "bad"
    ));
}

#[test]
fn rejects_mutual_reference() {
    let theory = theory_with(
        r#"
        (def proof first : wff -> wff = second)
        (def proof second : wff -> wff = first)
        "#,
    );
    for name in ["first", "second"] {
        assert!(matches!(
            check_definition(&theory, name),
            Err(Error::CyclicDefinition { .. })
        ));
    }
}

#[test]
fn rejects_transitive_cycles_and_reports_a_cycle_member() {
    let theory = theory_with(
        r#"
        (def proof wrapper : wff -> wff = first)
        (def proof first : wff -> wff = second)
        (def proof second : wff -> wff = third)
        (def proof third : wff -> wff = first)
        "#,
    );
    assert!(matches!(
        check_definition(&theory, "wrapper"),
        Err(Error::CyclicDefinition { operation })
            if matches!(operation.as_str(), "first" | "second" | "third")
    ));
}

#[test]
fn accepts_forward_references_and_shared_dependencies() {
    let theory = theory_with(
        r#"
        (def proof first : wff -> wff = (left right left))
        (def proof left : wff -> wff = shared)
        (def proof right : wff -> wff = shared)
        (def proof shared : wff -> wff = id)
        (def proof identity : wff -> wff = _)
        "#,
    );
    for name in ["first", "left", "right", "shared", "identity"] {
        check_definition(&theory, name).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}

#[test]
fn unrelated_cycles_do_not_affect_a_proof() {
    let theory = theory_with(
        r#"
        (def proof bad : wff -> wff = bad)
        (def proof good : wff -> wff = id)
        "#,
    );
    check_definition(&theory, "good").unwrap();
}

#[test]
fn missing_transitive_dependencies_return_an_error() {
    let mut theory = theory_with(
        r#"
        (def proof first : wff -> wff = second)
        (def proof second : wff -> wff = id)
        "#,
    );
    let Theory::Theory { arrows, .. } = &mut theory else {
        unreachable!();
    };
    arrows.remove(&"id".parse().unwrap());
    assert!(matches!(
        check_definition(&theory, "first"),
        Err(Error::InvalidTypeMaps)
    ));
}

#[test]
fn handles_long_dependency_chains_iteratively() {
    let mut definitions = String::new();
    for index in 0..2048 {
        definitions.push_str(&format!(
            "(def proof step-{index} : wff -> wff = step-{})\n",
            index + 1
        ));
    }
    definitions.push_str("(def proof step-2048 : wff -> wff = id)");
    let theory = theory_with(&definitions);
    check_definition(&theory, "step-0").unwrap();
}
