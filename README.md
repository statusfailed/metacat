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
[✓] p1 : wff -> {[ph . ] ([ . ph ph] -> [id . ]) ([ . id ph] -> [x . ph x] -> |-)}
[✓] p2 : wff -> ([ph . ph ph] -> [i . ph i] -> |-)
[✓] win : {wff wff} -> (-> -. wff)
[✓] p3 : wff -> {[ph . ] ([ . ph ph] -> [id . ]) ([ . id ph] -> [x . ph x] -> [lhs . ]) ([ . ph id] -> [y . y id] -> [rhs . ]) ([ . lhs rhs] -> |-)}
[✓] p4 : wff -> {[ph . ] ([ . ph ph] -> [id . ph id] -> [x . x id] -> |-)}
[✓] id : wff -> ([x . x x] -> |-)
```

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

See [./examples/fol.hex](./examples/fol.hex) for an example of declaring axioms of propositional logic,
and using them to prove the identity theorem `|- (φ → φ)`.

# inspection

The `inspect` commands expose the intermediate structures that metacat builds
from a `.hex` file.

Inspection stages correspond to different points in the checker pipeline:

- the default arrow inspector shows an arrow's type using the user-facing
  operational terms: the source is interpreted as a `match`, the target as a
  `build`, and the implementation is either primitive or a proof body.
- `term` is the interpreted proof body as an open hypergraph over proof arrows such as `wi`, `wn`, or `ax-mp`.
- `raw-type-map` is the operational hypergraph after composing the source,
  proof, and target pieces, but before the final quotient that glues equal
  nodes together.
- `type-map` is the operational hypergraph that the checker evaluates. It expands proof arrows into syntax constructors and matchers, giving the chain `source+ ; proof-as-type-map ; target-`.
- `ssa` is the topological order used to evaluate the `type-map`. It is a textual execution order, not a separate hypergraph.

List the declarations metacat parsed from a file:

```sh
metacat inspect declarations examples/fol.hex
```

Inspect a proof definition as an open hypergraph over proof arrows:

```sh
metacat inspect arrow examples/fol.hex win --stage term
```

Inspect an arrow's match/build type and implementation:

```sh
metacat inspect arrow examples/expressions.hex add-op
```

Inspect the operational type-map used by the checker:

```sh
metacat inspect arrow examples/fol.hex win --stage type-map
```

Inspect the same chain before the final quotient pass:

```sh
metacat inspect arrow examples/fol.hex win --stage raw-type-map
```

This is the hypergraph for the chain

```text
source+ ; proof-as-type-map ; target-
```

and its edges are labelled by syntax constructors and matchers such as
`fwd(wff)`, `fwd(->)`, `rev(wff)`, and `rev(->)`.

Inspect the topological order used for evaluation:

```sh
metacat inspect arrow examples/fol.hex win --stage ssa
```

Trace the checker step by step, including the tree values flowing through the
type-map:

```sh
metacat inspect check examples/fol.hex win --trace
```

For graph visualization, emit Graphviz DOT for hypergraph stages:

```sh
metacat inspect arrow examples/fol.hex win --stage type-map --format dot
```

The `raw-type-map` DOT output is clustered by default into `source+`,
`proof type-map`, and `target-`, with dotted edges showing pending node
identifications before quotienting.

For example, render the operational type-map to SVG:

```sh
metacat inspect arrow examples/fol.hex win --stage type-map --format dot \
  | dot -Tsvg > /tmp/win-type-map.svg
```

DOT output is available for `term`, `raw-type-map`, and `type-map`. The `ssa`
stage is a textual linearization rather than a hypergraph.

# tooling

See [metacat.nvim](https://github.com/statusfailed/metacat.nvim) for a simple
nvim plugin that will render proofs as SVGs.
