use metacat::check::{Error, check};
use metacat::theory::{Theory, TheoryId, TheorySet};

fn eta_mu_counterexample_result() -> Result<(), Error<hexpr::Operation>> {
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

          # Current checker bug: after global quotienting, this is accepted as
          # the identity 1 -> 1. With local spider execution, it should reject:
          # eta-id creates a fresh value and mu tries to merge it with b.
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
fn eta_mu_counterexample_is_rejected() {
    assert!(
        eta_mu_counterexample_result().is_err(),
        "eta/mu counterexample was accepted"
    );
}
