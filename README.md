# metacat

A theorem prover inspired by [metamath zero](https://github.com/digama0/mm0)
using [open-hypergraphs](https://github.com/hellas-ai/open-hypergraphs) for syntax.

# usage

Check all definitions in a file:

    metacat check <filename>

for example:

    > metacat check fol.hex
    [✓] p1 : wff -> {[ph . ] ([ . ph ph] -> [id . ]) ([ . id ph] -> [x . ph x] -> |-)}
    [✓] p2 : wff -> ([ph . ph ph] -> [i . ph i] -> |-)
    [✓] win : {wff wff} -> (-> -. wff)
    [✓] p3 : wff -> {[ph . ] ([ . ph ph] -> [id . ]) ([ . id ph] -> [x . ph x] -> [lhs . ]) ([ . ph id] -> [y . y id] -> [rhs . ]) ([ . lhs rhs] -> |-)}
    [✓] p4 : wff -> {[ph . ] ([ . ph ph] -> [id . ph id] -> [x . x id] -> |-)}
    [✓] id : wff -> ([x . x x] -> |-)

# language

A metacat file is a list of [hexprs](https://github.com/hellas-ai/hexpr), each
one of three kinds:

- **object declarations** `(object <name> : <arity> -> <coarity>)` declares a *generating object*
- **arrow declarations** `(arrow <name> : <src_hexpr> -> <tgt_hexpr>)` declares a *generating arrow* (an *axiom* or *inference rule*)
- **arrow definitions** `(def-arrow <name> : <src_hexpr> -> <tgt_hexpr> = <proof_hexpr>)` declares an arrow and gives a proof

For example, `fol.hex` declares the following objects:

    # well-formedness
    (object wff : 1 -> 1)

    # provability
    (object |- : 1 -> 1)

    # the "not" relation
    (object -. : 1 -> 1)

    # implication
    (object -> : 2 -> 1)

we can then make some axiomatic declarations that implication and negation are well-formed:

    # The negation of a wff is a wff
    (arrow wn : wff -> (-. wff))

    # implication of two wffs is a wff
    (arrow wi : {wff wff} -> (-> wff))

We can then write a simple proof that negation of implication is also well-formed:

    # ¬ (φ → ψ) is well-formed
    (def-arrow win : { wff wff } -> (-> -. wff) = (wi wn))

See [./fol.hex](./fol.hex) for an example of declaring axioms of propositional logic,
and using them to prove the identity theorem `|- (φ → φ)`.

# nvim-metacat

Use the `nvim-metacat` plugin in this repo to render diagrams of `def-arrow` terms from nvim.
To configure using [Lazy.nvim](https://github.com/folke/lazy.nvim), add this to `plugins.lua`:

    {
      "statusfailed/metacat/nvim-metacat",
      opts = {
        viewer = { "feh", "--reload", "1" },
      },
    },

You can then bind the `render` method to a key, e.g. F6 by adding the following to `init.lua`:

    vim.keymap.set("n", "<F6>", require("metacat").render, { desc = "Metacat: render def-arrow as SVG" })
