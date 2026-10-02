# Disjoint Variables / Admissible Reachability

This doc codifies syntax for specifying disjoint variable conditions in metacat .hex files.
The primary contribution is to add an optional `dv` annotation to `def`/`arr` declarations
which specifies the "admissible reachability" relation
(see the `metacategories` paper for theoretical details).

    (arr $NAME (dv $DV) : $SRC -> $TGT)
    (def $NAME (dv $DV) : $SRC -> $TGT = $DERIVATION)

The basic idea is to express the `ar : B -> C` relation as a morphism in FinRel,
as presented by a special bicommutative bimonoid

    dup : 1 -> 2    del : 1 -> 0
    sup : 2 -> 1    sel : 0 -> 1

So for example the relation for vars `x, y` and `ph : wff x` is given by `{_ del}`.
For something more complicated like `ph : wff x` `ps : wff x y` we'd have

    ({dup _} {_ sup})

We also allow using special frobenius structure to encode dup/del maps, so
the condition above could also be written

    ([x y . x x y] {_ sup})

# Examples

The axiom `ax-5` is currently written as below

    (arr ax-5 : ([x ph . ph] wff) -> ([x ph.] { [.ph] ([.x ph] forall) } -> |-))

With the DV syntax, there is one var and one formula. So we write

    (arr ax-5 (dv (del sel)) : ([x ph . ph] wff) -> ([x ph.] { [.ph] ([.x ph] forall) } -> |-))

Where `(del sel)` discards the single var, saying that it cannot appear in `wff`

Another example: consider `ax-12` in mm0:

    axiom ax_12 {x: nat} (a: nat) (p: wff x):
      $ x = a -> p -> A. x (x = a -> p) $;

In metacat proposed syntax, the dv condition is

    {_ del}

Since x may appear in p, but y must not.

Thus the general syntax is

    (arr $NAME : $SRC -> $TGT)
    (def $NAME : $SRC -> $TGT = $DERIVATION)

Disjoint variable annotatoins can be added optionally as

    (arr $NAME (dv $DV) : $SRC -> $TGT)
    (def $NAME (dv $DV) : $SRC -> $TGT = $DERIVATION)
