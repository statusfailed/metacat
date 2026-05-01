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

See [./examples/fol.hex](./examples/fol.hex) for an example of declaring axioms of propositional logic,
and using them to prove the identity theorem `|- (φ → φ)`.

# arrows

The `arrow` command prints or renders a named `def-arrow`.

Print the definition body as a hexpr:

```sh
metacat arrow hexpr examples/fol.hex win
```

Render the definition body to SVG:

```sh
metacat arrow svg examples/fol.hex win > /tmp/win.svg
```

Use `--orientation tb` for a top-to-bottom layout:

```sh
metacat arrow svg examples/fol.hex win --orientation tb > /tmp/win.svg
```

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
  nodes together. More precisely, this is the raw checker type term
  `source+ ; proof-type-map ; target-`.
- `type-term` is the quotient of that checker type term:
  `source+ ; proof-type-map ; target-`. The proof body is recursively expanded
  through `def-arrow`s first; every remaining primitive `arrow` is expanded into
  its match/build type-map interface.
- `proof-type-map` is the proof body's type map: proof arrows are replaced by
  their match/build interfaces, without adding the declaration source and
  target checks. The proof body is recursively expanded through `def-arrow`s
  first; every remaining primitive `arrow` is expanded into its match/build
  type-map interface.
- `ssa` is the topological order used to evaluate the checker type term. It is
  a textual execution order, not a separate hypergraph.

List the declarations metacat parsed from a file:

```sh
metacat inspect declarations examples/fol.hex
```

Inspect a proof definition as an open hypergraph over proof arrows:

```sh
metacat inspect arrow examples/fol.hex win --stage term
```

Inspect the declaration source or target object hypergraph:

```sh
metacat inspect arrow examples/expressions.hex assign-x-sum-times-one --stage source
metacat inspect arrow examples/expressions.hex assign-x-sum-times-one --stage target --format dot
```

Inspect an arrow's match/build type and implementation:

```sh
metacat inspect arrow examples/expressions.hex add
```

Inspect the checker type term `source+ ; proof-type-map ; target-`:

```sh
metacat inspect arrow examples/fol.hex win --stage type-term
```

Inspect only the proof body's type map:

```sh
metacat inspect arrow examples/fol.hex win --stage proof-type-map
```

Inspect the full checker chain before the final quotient pass:

```sh
metacat inspect arrow examples/fol.hex win --stage raw-type-map
```

This is the hypergraph for the checker chain

```text
source+ ; proof-type-map ; target-
```

and its edges are labelled by syntax constructors and matchers such as
`fwd(wff)`, `fwd(->)`, `rev(wff)`, and `rev(->)`.

Inspect the topological order used for evaluation:

```sh
metacat inspect arrow examples/fol.hex win --stage ssa
```

Trace the checker step by step, including the tree values flowing through the
checker type term:

```sh
metacat inspect check examples/fol.hex win --trace
```

For graph visualization, emit Graphviz DOT for hypergraph stages:

```sh
metacat inspect arrow examples/fol.hex win --stage type-term --format dot
```

For a compact composition-only view, emit the formula form:

```sh
metacat inspect arrow examples/expressions.hex assign-x-sum-times-one --stage type-term --format formula
```

This prints terms such as:

```text
source+({num num num}) ; add- ; add+ ; one+ ; mul- ; mul+ ; target-({num num num})
```

The `raw-type-map` DOT output is clustered by default into `source+`,
`proof type-map`, and `target-`, with dotted edges showing pending node
identifications before quotienting.

For example, render the checker type term to SVG:

```sh
metacat inspect arrow examples/fol.hex win --stage type-term --format dot \
  | dot -Tsvg > /tmp/win-type-term.svg
```

DOT output is available for `source`, `target`, `term`, `raw-type-map`,
`type-term`, and `proof-type-map`. The `ssa` stage is a textual linearization
rather than a hypergraph.

# tooling

See [metacat.nvim](https://github.com/statusfailed/metacat.nvim) for a simple
nvim plugin that will render proofs as SVGs.
