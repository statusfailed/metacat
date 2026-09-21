use metacat::check::check;
use metacat::theory::{Theory, TheoryId, TheorySet};

#[test]
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
    assert!(result.unwrap().proof_classes().is_injective());

    Ok(())
}

#[test]
fn self_typed_generator_checks() -> Result<(), Box<dyn std::error::Error>> {
    let theories = TheorySet::from_text(
        r#"
        (theory syntax nat {
          (arr wff : 1 -> 1)
          (arr -. : 1 -> 1)
        })

        (theory proof syntax {
          (arr wn : wff -> (-. wff))
          (def wn-self : wff -> (-. wff) = wn)
        })
        "#,
    )?;

    let proof_id = TheoryId("proof".parse()?);
    let proof_theory = theories.theories.get(&proof_id).unwrap();
    let Theory::Theory { arrows, .. } = proof_theory else {
        panic!("expected proof theory");
    };
    let proof = arrows.get(&"wn-self".parse()?).unwrap();
    let mut definition = proof.definition.clone().unwrap();

    let result = check(
        proof_theory,
        proof.type_maps.0.clone(),
        proof.type_maps.1.clone(),
        &mut definition,
    )?;
    assert!(result.proof_classes().is_injective());

    Ok(())
}
