use metacat::theory::{Term, Theory, TheoryId, TheorySet};

#[derive(Debug)]
struct DisabledChecker;

fn check(
    _theory: &Theory,
    _source: Term,
    _target: Term,
    _arrow: &mut Term,
) -> Result<Vec<()>, DisabledChecker> {
    panic!("checker v2 is disabled")
}

#[test]
#[ignore = "checker v2 is disabled"]
fn typed_wire_identity_checks() -> Result<(), Box<dyn std::error::Error>> {
    let theories = TheorySet::from_text(
        r#"
        (theory typed.syntax nat {
          (arr carrier : ^One -> ^One)
        })

        (theory typed.proof typed.syntax {
          (arr typed-id : ^A -> ^A)
          (def typed-id-proof : ^A -> ^A = [x^A])
        })
        "#,
    )?;

    let proof_id = TheoryId("typed.proof".parse()?);
    let proof_theory = theories.theories.get(&proof_id).unwrap();
    let Theory::Theory { arrows, .. } = proof_theory else {
        panic!("expected proof theory");
    };
    let proof = arrows.get(&"typed-id-proof".parse()?).unwrap();
    let mut definition = proof.definition.clone().unwrap();

    let result = check(
        proof_theory,
        proof.type_maps.0.clone(),
        proof.type_maps.1.clone(),
        &mut definition,
    );
    assert!(
        result.is_ok(),
        "typed-wire identity should check: {result:?}"
    );

    Ok(())
}

fn eta_mu_counterexample_result() -> Result<(), DisabledChecker> {
    let theories = TheorySet::from_text(
        r#"
        (theory eta-mu.syntax nat {
          (arr atom : 1 -> 1)
        })

        (theory eta-mu.proof eta-mu.syntax {
          # eta-id : b -> {a b}; operationally, matching the source creates
          # a fresh metavariable a while passing b through.
          (arr eta-id : [a b . b] -> [a b])

          # mu : {a a} -> a; operationally, this should require its two inputs
          # to be equal before passing the common value through.
          (arr mu : [a . a a] -> [a])

          # The regression expects this to be rejected: eta-id creates a fresh
          # value and mu attempts to merge that value with b.
          (def eta-mu : [b] -> [b] = (eta-id mu))
        })
        "#,
    )
    .expect("test theory should load");

    let theory_id = TheoryId("eta-mu.proof".parse().expect("valid theory id"));
    let theory = theories
        .theories
        .get(&theory_id)
        .expect("test theory should exist");
    let Theory::Theory { arrows, .. } = theory else {
        panic!("test theory should be a user theory");
    };

    let declaration = arrows
        .get(&"eta-mu".parse().expect("valid operation"))
        .expect("test definition should exist");
    let mut term = declaration
        .definition
        .clone()
        .expect("test declaration should be definitional");
    let (source, target) = declaration.type_maps.clone();

    check(theory, source, target, &mut term).map(|_| ())
}

#[test]
#[ignore = "checker v2 is disabled"]
fn eta_mu_counterexample_is_rejected() {
    assert!(
        eta_mu_counterexample_result().is_err(),
        "eta/mu counterexample was accepted"
    );
}
