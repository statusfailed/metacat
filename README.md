# metacat

A theorem prover inspired by [metamath zero](https://github.com/digama0/mm0)
using [open-hypergraphs](https://github.com/hellas-ai/open-hypergraphs) for syntax.

# usage

Check all definitions in a theory across one or more files:

    metacat check <theory-name> <file1> <file2> ...

for example:

    > metacat check fol.proof fol.hex
    [✓] p1 : wff -> {[ph . ] ([ . ph ph] -> [id . ]) ([ . id ph] -> [x . ph x] -> |-)}
    [✓] p2 : wff -> ([ph . ph ph] -> [i . ph i] -> |-)
    [✓] win : {wff wff} -> (-> -. wff)
    [✓] p3 : wff -> {[ph . ] ([ . ph ph] -> [id . ]) ([ . id ph] -> [x . ph x] -> [lhs . ]) ([ . ph id] -> [y . y id] -> [rhs . ]) ([ . lhs rhs] -> |-)}
    [✓] p4 : wff -> {[ph . ] ([ . ph ph] -> [id . ph id] -> [x . x id] -> |-)}
    [✓] id : wff -> ([x . x x] -> |-)

# language

A metacat file is a list of [hexprs](https://github.com/hellas-ai/hexpr),
typically top-level `theory` declarations:

- **theory declarations** `(theory <name> <syntax> { <decl>* })` declare a theory over some syntax category
- **arrow declarations** `(arr <name> : <src_hexpr> -> <tgt_hexpr>)` declare a *generating arrow* (an *axiom* or *inference rule*)
- **arrow definitions** `(def <name> : <src_hexpr> -> <tgt_hexpr> = <proof_hexpr>)` declare an arrow and give a proof

For example, `fol.hex` declares the following syntax theory:

    (theory fol.syntax nat {
      # well-formedness
      (arr wff : 1 -> 1)

      # provability
      (arr |- : 1 -> 1)

      # the "not" relation
      (arr -. : 1 -> 1)

      # implication
      (arr -> : 2 -> 1)
    })

We can then make some axiomatic declarations in a proof theory over that syntax:

    (theory fol.proof fol.syntax {
      # The negation of a wff is a wff
      (arr wn : wff -> (-. wff))

      # implication of two wffs is a wff
      (arr wi : {wff wff} -> (-> wff))
    })

We can then write a simple proof that negation of implication is also well-formed:

    (theory fol.proof fol.syntax {
      # ¬ (φ → ψ) is well-formed
      (def win : { wff wff } -> (-> -. wff) = (wi wn))
    })

See [./fol.hex](./fol.hex) for an example of declaring axioms of propositional logic,
and using them to prove the identity theorem `|- (φ → φ)`.

# tooling

See [metacat.nvim](https://github.com/statusfailed/metacat.nvim) for a simple
nvim plugin that will render proofs as SVGs.
