use metacat::check::{Error, check};
use metacat::theory::{Term, Theory, TheoryId, TheorySet};

const THEORY: &str = r#"
    (theory syntax nat {
      (arr true : 0 -> 1)
      (arr |- : 1 -> 1)
    })
    (theory proof syntax {
      (arr id : |- -> |-)
      (arr truth : {} -> (true |-))
      (arr copy : |- -> ([x.x x] {|- |-}))
      (arr erase : |- -> [x.])
      (arr scalar : {} -> {})
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
fn non_monogamous_proof_wiring_is_rejected() {
    let theory = theory_with(
        r#"
        (def proof create : {} -> (true |-) = [.p])
        (def proof discard : |- -> [x.] = [p.])
        (def proof duplicate : |- -> ([x.x x] {|- |-}) = [p.p p])
        (def proof merge : ([x.x x] {|- |-}) -> |- = [p p.p])
        (def proof fanout : |- -> ([x.x x] {|- |-}) = ([p.p p] {id id}))
        (def proof producers : {} -> (true |-) = ({truth truth} [p p.p]))
        (def proof feed-input : (true |-) -> (true |-) = {[p.] (truth [p.]) [.p]})
        (def proof read-output : |- -> |- = {[p.p] ([.p] erase)})
        "#,
    );
    for name in [
        "create",
        "discard",
        "duplicate",
        "merge",
        "fanout",
        "producers",
        "feed-input",
        "read-output",
    ] {
        let result = check_definition(&theory, name);
        assert!(
            matches!(result, Err(Error::NonMonogamousProof)),
            "{name}: {result:?}"
        );
    }
}

#[test]
fn isolated_proof_nodes_are_rejected() {
    let theory = theory_with("");
    let mut proof = Term::empty();
    proof.new_node(());
    assert!(matches!(
        check(&theory, Term::empty(), Term::empty(), &mut proof),
        Err(Error::NonMonogamousProof)
    ));
}

#[test]
fn monogamous_cycles_are_rejected_including_disconnected_components() {
    let theory = theory_with(
        r#"
        (def proof self-loop : {} -> {} = ([.p] id [p.]))
        (def proof longer-cycle : {} -> {} = {([.p] id [q.]) ([.q] id [p.])})
        (def proof disconnected-cycle : |- -> |- = {_ ([.p] id [p.])})
        "#,
    );
    for name in ["self-loop", "longer-cycle", "disconnected-cycle"] {
        let declaration = theory.get_arrow(&name.parse().unwrap()).unwrap();
        let proof = declaration.definition.clone().unwrap().to_strict();
        assert!(proof.is_monogamous(), "{name} must isolate the cycle check");
        assert!(!proof.is_acyclic());
        let result = check_definition(&theory, name);
        assert!(
            matches!(result, Err(Error::CyclicProof)),
            "{name}: {result:?}"
        );
    }
}

#[test]
fn pending_identifications_are_applied_before_structure_checks() {
    let theory = theory_with("");
    let mut proof = Term::empty();
    let start = proof.new_node(());
    let middle = proof.new_node(());
    let end = proof.new_node(());
    proof.new_edge("id".parse().unwrap(), ([start], [middle]));
    proof.new_edge("id".parse().unwrap(), ([middle], [end]));
    proof.unify(start, end);

    assert!(matches!(
        check(&theory, Term::empty(), Term::empty(), &mut proof),
        Err(Error::CyclicProof)
    ));
    assert_eq!(proof.hypergraph.nodes.len(), 2);
}

#[test]
fn monogamous_acyclic_proofs_allow_explicit_structural_generators() {
    let theory = theory_with(
        r#"
        (def proof empty : {} -> {} = {})
        (def proof identity : |- -> |- = _)
        (def proof permutation : {|- |-} -> ([x y.y x] {|- |-}) = [p q.q p])
        (def proof composition : |- -> |- = (id id))
        (def proof tensor : {|- |-} -> {|- |-} = {id id})
        (def proof constant : {} -> (true |-) = truth)
        (def proof copy-proof : |- -> ([x.x x] {|- |-}) = copy)
        (def proof erase-proof : |- -> [x.] = erase)
        (def proof copy-then-erase : |- -> |- = (copy {_ erase}))
        (def proof scalar-proof : {} -> {} = scalar)
        "#,
    );
    for name in [
        "empty",
        "identity",
        "permutation",
        "composition",
        "tensor",
        "constant",
        "copy-proof",
        "erase-proof",
        "copy-then-erase",
        "scalar-proof",
    ] {
        check_definition(&theory, name).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}
