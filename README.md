# metacat

A theorem prover inspired by [metamath zero](https://github.com/digama0/mm0)
using [open-hypergraphs](https://github.com/hellas-ai/open-hypergraphs) for syntax.

# usage

Check all definitions in a file:

```sh
metacat check <filename>
```

for example:

```text
> metacat check examples/fol.hex
[✓] ax5d : {[x ph ps . ] ([ . ph] wff) ([ . ps] wff)} -> {[x ph ps . ] ([ . x ps] forall [aps . ]) ([ . ps aps] -> [inner . ]) ([ . ph inner] -> |-)}
[✓] win : {wff wff} -> (-> -. wff)
[✓] id : wff -> ([x . x x] -> |-)
[✓] win-shared : {wff} -> ([x . x x] -> -. wff)
[✓] p2 : wff -> ([ph . ph ph] -> [i . ph i] -> |-)
[✓] p3 : wff -> {[ph . ] ([ . ph ph] -> [id . ]) ([ . id ph] -> [x . ph x] -> [lhs . ]) ([ . ph id] -> [y . y id] -> [rhs . ]) ([ . lhs rhs] -> |-)}
[✓] p4 : wff -> {[ph . ] ([ . ph ph] -> [id . ph id] -> [x . x id] -> |-)}
[✓] p1 : wff -> {[ph . ] ([ . ph ph] -> [id . ]) ([ . id ph] -> [x . ph x] -> |-)}
[✓] id-inline : wff -> ([x . x x] -> |-)
[✓] a1i : {[ph ps . ] ([ . ph] wff) ([ . ps] wff) ([ . ph] |-)} -> ([ph ps . ps ph] -> |-)
[✓] gen2 : {[x y ph . ] ([ . x] setvar) ([ . y] setvar) ([ . ph] wff) ([ . ph] |-)} -> {[x y ph . ] ([ . y ph] forall [yph . ]) ([ . x yph] forall |-)}
```

The output order is not significant.

# language

A metacat file is a list of [hexprs](https://github.com/hellas-ai/hexpr), each
one of three kinds:

- **object declarations** `(object <name> : <arity> -> <coarity>)` declares a _generating object_
- **arrow declarations** `(arrow <name> : <src_hexpr> -> <tgt_hexpr>)` declares a _generating arrow_ (an _axiom_ or _inference rule_)
- **arrow definitions** `(def-arrow <name> : <src_hexpr> -> <tgt_hexpr> = <proof_hexpr>)` declares an arrow and gives a proof

For example, `fol.hex` declares the following objects:

```hex
# well-formedness
(object wff : 1 -> 1)

# provability
(object |- : 1 -> 1)

# the "not" relation
(object -. : 1 -> 1)

# implication
(object -> : 2 -> 1)
```

we can then make some axiomatic declarations that implication and negation are well-formed:

```hex
# The negation of a wff is a wff
(arrow wn : wff -> (-. wff))

# implication of two wffs is a wff
(arrow wi : {wff wff} -> (-> wff))
```

We can then write a simple proof that negation of implication is also well-formed:

```hex
# ¬ (φ → ψ) is well-formed
(def-arrow win : { wff wff } -> (-> -. wff) = (wi wn))
```

See [./fol.hex](./fol.hex) for an example of declaring axioms of propositional logic,
and using them to prove the identity theorem `|- (φ → φ)`.

# tooling

See [metacat.nvim](https://github.com/statusfailed/metacat.nvim) for a simple
nvim plugin that will render proofs as SVGs.
