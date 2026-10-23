# Syntactic Definitions

This document describes the motivation, design, and pitfalls of introducing
first-class support for "user definitions" in proofs.

## Motivation

It is often convenient to introduce a new symbol as shorthand for a more complex equation.
For instance, we can define `and` as below.

    and(φ, ψ) := ¬(φ → ¬ψ)

In the same way it is convenient in a proof to replace a lemma by its statement
-- and check the lemma separately -- it is similarly convenient to state a complex
property in terms of shorthand definitions.
This becomes more beneficial as definitions become more complex, for example
the definition of continuity at a point (below).

  continuous-at(f, a) :=
    ∀ ε > 0. ∃ δ > 0. ∀ x ∈ ℝ.
      |x − a| < δ → |f(x) − f(a)| < ε

There are multiple benefits to writing `continuous-at` instead of its full definition:

1. The proof statement is more readable
2. The proof statement is *shorter* (inlining *all* definitions can potentially lead to *much* larger code)
3. One can hide existential variables behind the definition

With regards to (3), notice that we can write a definition like "nonempty" below.

    nonempty(A) :=  ∃x. x ∈ A

Here, when we write `nonempty` in a proof, we need not let the existential
binder metavar `x` clutter our proof; we can simply ignore it until it's time
to unpack the definition.

So how do we work with such definitions?

## Manual Definitions and Trust

In Metamath one simply introduces a definition manually.
For example, we introduce a symbol `nonempty : 1 -> 1`,
and separately an axiom

    df-nonempty : () -> |- nonempty(A) ↔ ∃ x . x ∈ A

Mechanically, this is fine; the problem is *trust*.

- If we wish to allow a *user* to introduce such a definition, then the user
  must be allowed to declare axioms.
- If the user can declare axioms, user axioms can make our logic inconsistent/unsound
- Thus we have to check user code

This state of affairs is unacceptable!
We therefore wish to permit users to make this kind of definition,
but limited only to "trustable" forms.

This problem is addressed in Metamath Zero by first-class support for definitions.
We emulate this approach, as detailed in the next section.

## Definitions

For the purposes of first order logic, the general form of a definition is below,
module metavariable ordering.

    |- def-name(x₀, x₁, ... xₙ) ↔ def-body(x₀, x₁, ... xₙ, xₙ₊₁, ..., xₙ₊ₘ)

In general though, notice that:

- The definition requires two things:
    1. A symbol `def-name` in the syntax category
    2. The *axiom* witnessing `def-name ↔ def-body` in the proof category
- The `def-name` syntax has a *subset* of metavariables appearing in the body

This last point leads us to a problem with our first attempt...

## Syntactic definitions as `def` arrows in the syntax category

We already have a means for making definitions in a syntax category.
For instance, we could write a definition called `and` as below:

    # and(φ, ψ) := ¬(φ → ¬ψ)
    (def and : 2 -> 1 = ({_ ¬} → ¬))

Currently, metacat permits us to use this as any other syntactic form.
But without elaboration, we are missing the *axiom* that witnesses 
the equivalent of `and(φ, ψ)` with its body `¬(φ → ¬ψ)`.

A straightforward approach is to automatically elaborate the proof theory with
an arrow:

    # add the biconditional and(φ, ψ) ↔ ¬(φ → ¬ψ))
    # note: written in shorthand!
    (arr def.and : () -> |- and(φ, ψ) ↔ ¬(φ → ¬ψ))

However, this approach has two main deficiencies:

1. A syntax-category `def` must have the same arity/coarity as its body, and thus cannot hide metavariables
2. There is no way to introduce definitions with disjoint variable conditions

The former prevents us from hiding a metavariable in the shorthand, as for
example in `nonempty`:

    nonempty(A) :=  ∃x. x ∈ A

The latter prevents us from writing definitions such as the below,
where we require `x # y`:

    two-values-exist := ∃ x ∃ y . x ≠ y     # for x # y

## Solution

TODO

# Implementation Details

The implementation of syntactic definitions is somewhat tied to the logic being checked.
For instance, in first order logic we may wish to add a *biconditional* as the axiom,
but other formal systems may differ.

For this reason, the syntactic definition elaboration is implemented as a
"pluggable feature" in the checker,
while the `metacat` library comes with a default implementation suitable for
FOL.
