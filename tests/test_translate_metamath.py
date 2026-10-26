"""Run with: python3 -m unittest discover -s tests -v."""
import itertools
from collections import Counter
from pathlib import Path
import re
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import translate_metamath as mm


PROPOSITIONAL = r"""
$c wff |- ( ) -> $.
$v ph ps $.
wph $f wff ph $.
wps $f wff ps $.
wi $a wff ( ph -> ps ) $.
"""


class MetamathTranslationTests(unittest.TestCase):
    def assert_monogamous_wires(self, emitted):
        producers, consumers = Counter(), Counter()
        for source, target in re.findall(r"\[([^\[\].]*)\.([^\[\].]*)\]", emitted):
            producers.update(source.split())
            consumers.update(target.split())
        self.assertEqual(producers, consumers)
        self.assertTrue(all(count == 1 for count in producers.values()))

    def test_compressed_proof_preserves_saved_subproof_sharing(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            shared $p wff ( ( ph -> ph ) -> ( ph -> ph ) ) $=
                ( wi ) AABZCB $.
        """)
        root = mm.prove(db, db["shared"])
        self.assertIs(root.arguments[0], root.arguments[1])
        self.assertEqual(root.expression, db["shared"].expression)
        emitted = mm.proof_hex(db, db["shared"], root, aliases={})
        self.assertEqual(emitted.count(" wi "), 2)
        self.assertEqual(emitted.count(" mm-copy-wff "), 2)
        self.assert_monogamous_wires(emitted)

    def test_unused_hypotheses_are_explicitly_discarded(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            ${ h $e |- ph $. unused $e |- ps $.
               keep $p |- ph $= h $.
            $}
        """)
        emitted = mm.proof_hex(db, db["keep"], mm.prove(db, db["keep"]), aliases={})
        self.assertEqual(emitted.count("mm-discard-wff"), 2)
        self.assertEqual(emitted.count("mm-discard-proof"), 1)
        self.assertNotIn("mm-copy", emitted)
        self.assert_monogamous_wires(emitted)

    def test_repeated_essential_hypotheses_use_proof_copying(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            ${ first $e |- ph $. second $e |- ph $. both $a |- ph $. $}
            ${ h $e |- ph $. repeated $p |- ph $= wph h h both $. $}
        """)
        emitted = mm.proof_hex(db, db["repeated"], mm.prove(db, db["repeated"]), aliases={})
        self.assertEqual(emitted.count(" mm-copy-proof "), 1)
        self.assert_monogamous_wires(emitted)

    def test_large_fanout_and_identity_have_distinct_use_wires(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            identity $p wff ph $= wph $.
            many $p wff ( ( ph -> ph ) -> ( ph -> ( ph -> ph ) ) ) $=
                wph wph wi wph wph wph wi wi wi $.
        """)
        for name in ("identity", "many"):
            emitted = mm.proof_hex(db, db[name], mm.prove(db, db[name]), aliases={})
            self.assert_monogamous_wires(emitted)
            if name == "identity":
                self.assertNotIn("mm-copy", emitted)
                self.assertNotIn("mm-discard", emitted)

    def test_generated_helper_names_cannot_shadow_source_assertions(self):
        for name in sorted(mm.GENERATED_HELPERS):
            with self.subTest(name=name), self.assertRaisesRegex(mm.TranslationError, "conflicts"):
                db = mm.parse_database(PROPOSITIONAL + f"{name} $a |- ph $.")
                mm.generate_all(db, "test")

    def test_uncompressed_and_multidigit_compressed_proofs(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            plain $p wff ( ph -> ph ) $= wph wph wi $.
        """)
        theorem = db["plain"]
        expected = mm.prove(db, theorem).expression
        # One mandatory hypothesis plus 21 explicit labels: UB encodes 22.
        theorem.proof = ("(",) + ("wi",) * 21 + (")", "AAUB")
        self.assertEqual(mm.prove(db, theorem).expression, expected)

    def test_rejects_invalid_compressed_proofs_and_stack_underflow(self):
        for proof in ("wi", "unknown", "( wi ) ?", "( wi ) Z", "( wi ) C",
                      "( wi ) U", "( wi ) AUZ", "( wi AAB"):
            with self.subTest(proof=proof), self.assertRaises(mm.TranslationError):
                db = mm.parse_database(PROPOSITIONAL + f"bad $p wff ph $= {proof} $.")
                mm.prove(db, db["bad"])

    def test_rejects_forward_self_and_out_of_scope_references(self):
        cases = (
            "bad $p wff ph $= wph later $. later $a wff ph $.",
            "bad $p wff ph $= wph bad $.",
            "${ secret $e wff ph $. $} bad $p wff ph $= secret $.",
        )
        for text in cases:
            with self.subTest(text=text), self.assertRaises(mm.TranslationError):
                db = mm.parse_database(PROPOSITIONAL + text)
                mm.prove(db, db["bad"])

    def test_verifies_essential_hypotheses_and_floating_types(self):
        source = PROPOSITIONAL + r"""
            truth $a |- ph $.
            ${ h $e |- ph $. keep $a |- ph $. $}
        """
        for proof in ("wph wph keep", "wph truth wph wi"):
            with self.subTest(proof=proof), self.assertRaises(mm.TranslationError):
                db = mm.parse_database(source + f"bad $p |- ph $= {proof} $.")
                mm.prove(db, db["bad"])

    def test_verifies_disjointness_and_restores_scope(self):
        source = r"""
            $c setvar |- = $.
            $v x y $.
            vx $f setvar x $. vy $f setvar y $.
            ${ $d x y $. apart $a |- x = y $. $}
            ${ $d x y $. good $p |- x = y $= vx vy apart $. $}
            bad $p |- x = y $= vx vy apart $.
            same $p |- x = x $= vx vx apart $.
        """
        db = mm.parse_database(source)
        mm.prove(db, db["good"])
        for name in ("bad", "same"):
            with self.subTest(name=name), self.assertRaisesRegex(mm.TranslationError, "requires \\$d"):
                mm.prove(db, db[name])

    def test_grammar_is_derived_and_uses_hypothesis_order(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            $c choose $.
            choice $a wff choose ps ph $.
        """)
        grammar = mm.SyntaxGrammar(db)
        grammar.variables = {"ph": "wff", "ps": "wff"}
        self.assertEqual(grammar.parse(("wff", "choose", "ps", "ph")),
                         ("mm-choice", "ph", "ps"))
        self.assertEqual(grammar.parse(("wff", "(", "ph", "->", "ps", ")")),
                         ("mm-wi", "ph", "ps"))
        with self.assertRaises(mm.TranslationError):
            grammar.parse(("wff", "unknown", "ph"))

    def test_inlining_reuses_actual_hypotheses(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            alias $p wff ( ph -> ps ) $= wph wps wi $.
            use $p wff ( ph -> ph ) $= wph wph alias $.
        """)
        translator = mm.ProofTranslator(db, max_steps=100)
        translator.bare["wi"] = []
        root, inlined = translator.expand(db["use"], mm.prove(db, db["use"]))
        self.assertEqual(inlined, ["alias"])
        self.assertEqual(root.label, "wi")
        self.assertIs(root.arguments[0], root.arguments[1])
        self.assertEqual(root.expression, db["use"].expression)

    def test_bare_variables_are_inferred_without_adding_distinctness(self):
        db = mm.parse_database(r"""
            $c setvar |- = $.
            $v x y $. vx $f setvar x $. vy $f setvar y $.
            ax $a |- x = y $.
            derived $p |- x = y $= vx vy ax $.
        """)
        translator = mm.ProofTranslator(db, max_steps=100)
        translator.bare["ax"] = []
        root = mm.prove(db, db["derived"])
        self.assertEqual(translator.requirements(db["derived"], root), [])
        translator.bare["ax"] = ["x"]
        self.assertEqual(translator.requirements(db["derived"], root), ["x"])
        self.assertEqual(list(mm.unbundled_patterns(["x", "y"], ["x"], ())), [(0, 1), (0, 0)])

    def test_identical_applications_share_but_local_variables_stay_fresh(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            duplicate $p wff ( ( ph -> ph ) -> ( ph -> ph ) ) $=
                wph wph wi wph wph wi wi $.
            $c setvar $. $v x y $.
            vx $f setvar x $. vy $f setvar y $.
            ${ h $e setvar x $. discard $a wff ph $. $}
            hide $p wff ph $= wph vy vy discard $.
            use $p wff ( ph -> ps ) $= wph hide wps hide wi $.
        """)
        translator = mm.ProofTranslator(db, max_steps=100)
        translator.bare.update(wi=[], discard=[])
        root, _ = translator.expand(db["duplicate"], mm.prove(db, db["duplicate"]))
        self.assertIs(root.arguments[0], root.arguments[1])
        root, _ = translator.expand(db["use"], mm.prove(db, db["use"]))
        locals_ = [step for step in mm.proof_steps(root) if step.local]
        self.assertEqual(len(locals_), 2)
        self.assertNotEqual(locals_[0].expression, locals_[1].expression)

    def test_minimal_primitive_partition_still_covers_every_dv_pair(self):
        db = mm.parse_database(r"""
            $c setvar |- = $. $v x y $.
            vx $f setvar x $. vy $f setvar y $.
            ${ $d x y $. apart $a |- x = y $. $}
        """)
        translator = mm.ProofTranslator(db, max_steps=100)
        bare = translator.requirements(db["apart"], None)
        self.assertEqual(len(bare), 1)
        self.assertTrue(all(set(pair) & set(bare) for pair in db["apart"].dv))

    def test_primitive_cover_prefers_no_unbundling_when_possible(self):
        db = mm.parse_database(r"""
            $c setvar |- = $. $v x y z $.
            vx $f setvar x $. vy $f setvar y $. vz $f setvar z $.
            ${ $d x y $. $d x z $. h $e setvar z $. apart $a |- x = y $. $}
        """)
        translator = mm.ProofTranslator(db, max_steps=100)
        self.assertEqual(translator.requirements(db["apart"], None), ["x"])

    def test_applications_select_distinct_and_identified_variants(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            $c setvar A. $. $v x y $.
            vx $f setvar x $. vy $f setvar y $.
            ${ $d ph x $. guard $a |- A. x ph $. $}
            ${ h $e setvar y $. $d ph x $.
              wrapper $p |- A. x ph $= wph vx guard $.
            $}
            ${ $d ph x $.
              same $p |- A. x ph $= wph vx vx vx wrapper $.
            $}
            ${ $d ph x $. $d x y $.
              different $p |- A. x ph $= wph vx vy vy wrapper $.
            $}
        """)
        translator = mm.ProofTranslator(db, max_steps=100)
        variants = {p: mm.specialize_statement(translator.database, db["wrapper"], ["x"], p)
                    for p in ((0, 1), (0, 0))}
        translator.bare["guard"] = ["x"]
        translator.required_bare["wrapper"] = ["x"]
        for variant in variants.values():
            translator.bare[variant.statement.label] = variant.bare
        translator.variants["wrapper"] = variants
        same, _ = translator.expand(db["same"], mm.prove(db, db["same"]))
        different, _ = translator.expand(db["different"], mm.prove(db, db["different"]))
        self.assertEqual(same.label, variants[(0, 0)].statement.label)
        self.assertEqual(different.label, variants[(0, 1)].statement.label)
        self.assertEqual(len(same.arguments), 3)  # duplicate floating input removed
        self.assertEqual(len(different.arguments), 4)

    def test_patterns_cover_all_and_only_compatible_bare_identifications(self):
        variables = ["x", "y", "z"]
        patterns = list(mm.unbundled_patterns(variables, variables, ()))
        self.assertEqual(len(patterns), 5)  # Bell number B_3
        self.assertEqual(len(set(patterns)), 5)
        patterns = list(mm.unbundled_patterns(variables, ["y"], {("x", "z")}))
        self.assertEqual(set(patterns), {(0, 1, 2), (0, 0, 2), (0, 1, 1)})
        self.assertEqual(list(mm.unbundled_patterns(variables, [], ())), [(0, 1, 2)])
        # C-only coincidences are handled by substitution, not extra variants.
        self.assertEqual(mm.application_pattern(variables, ["x"],
                                               {"x": ("a",), "y": ("b",), "z": ("b",)}), (0, 1, 2))

    def test_patterns_match_exhaustive_four_variable_partition_model(self):
        variables = ["w", "x", "y", "z"]
        pairs = list(itertools.combinations(variables, 2))
        # An independent restricted-growth enumeration of all set partitions.
        partitions = [p for p in itertools.product(range(4), repeat=4)
                      if p[0] == 0 and all(p[i] <= 1 + max(p[:i]) for i in range(1, 4))]
        self.assertEqual(len(partitions), 15)
        for bare_mask in range(16):
            bare = [v for i, v in enumerate(variables) if bare_mask & (1 << i)]
            for dv_mask in range(64):
                dv = {pair for i, pair in enumerate(pairs) if dv_mask & (1 << i)}
                expected = set()
                for p in partitions:
                    if any(p[variables.index(x)] == p[variables.index(y)] for x, y in dv):
                        continue
                    bound_classes = {p[variables.index(v)] for v in bare}
                    expected.add(tuple(p.index(c) if c in bound_classes else i for i, c in enumerate(p)))
                actual = list(mm.unbundled_patterns(variables, bare, dv))
                self.assertEqual(set(actual), expected)
                self.assertEqual(len(actual), len(expected))

    def test_specialization_quotients_hypotheses_and_renames_essential_inputs(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            $c setvar A. $. $v x y $. vx $f setvar x $. vy $f setvar y $.
            ${ h $e |- A. y ph $. keep $a |- A. y ph $. $}
            ${ h2 $e |- A. y ph $. hx $e setvar x $.
               use $p |- A. y ph $= wph vy h2 keep $.
            $}
        """)
        source = db["use"]
        variant = mm.specialize_statement(db, source, ["y"], (0, 0))
        root = mm.specialize_proof(mm.prove(db, source), variant)
        self.assertEqual(variant.bare, ["x"])
        self.assertEqual(mm.mandatory_variables(db, variant.statement), ["ph", "x"])
        self.assertEqual(root.expression, ("|-", "A.", "x", "ph"))
        self.assertEqual(root.arguments[2].label, variant.hypotheses["h2"])
        self.assertEqual(root.arguments[2].expression, ("|-", "A.", "x", "ph"))
        self.assertEqual(root.arguments[1].label, variant.hypotheses["vx"])

    def test_generation_unbundles_proofs_and_generates_demanded_cases_past_limit(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            $c setvar A. $. $v x y $. vx $f setvar x $. vy $f setvar y $.
            al $a wff A. x ph $.
            ${ $d ph x $. guard $a |- A. x ph $. $}
            ${ h $e setvar y $. $d ph x $.
               wrapper $p |- A. x ph $= wph vx guard $.
            $}
            ${ $d ph x $. same $p |- A. x ph $= wph vx vx vx wrapper $. $}
        """)
        _, pieces, records = mm.generate_all(db, "test", max_variants=1)
        variants = records["wrapper"]["variants"]
        self.assertEqual(len(variants), 2)  # second case requested by same
        self.assertTrue(records["wrapper"]["enumeration_complete"])
        self.assertNotIn("wrapper", pieces)
        self.assertIn(variants[1], records["same"]["dependencies"])
        self.assertLess(list(pieces).index(variants[1]), list(pieces).index("same"))
        self.assertTrue(all("(def " + name in pieces[name] for name in variants))
        _, limited, incomplete = mm.generate_all(db, "test", through="wrapper", max_variants=1)
        self.assertEqual(incomplete["wrapper"]["status"], "unbundling_limited")
        self.assertNotIn(variants[1], limited)
        _, complete, unlimited = mm.generate_all(db, "test", through="wrapper", max_variants=0)
        self.assertTrue(unlimited["wrapper"]["enumeration_complete"])
        self.assertTrue(set(variants) <= complete.keys())
        # A failed specialization is inlined, never silently replaced by a
        # distinct-variable case or promoted to an axiom.
        _, fallback, excluded = mm.generate_all(db, "test", excluded=[variants[1]])
        self.assertNotIn(variants[1], fallback)
        self.assertEqual(excluded["same"]["dependencies"], ["guard"])
        self.assertEqual(excluded["wrapper"]["status"], "translation_failed")

    def test_incomplete_or_failed_families_are_not_counted_as_full_ports(self):
        records = {
            "family": {"kind": "$p", "variants": ["distinct", "same"], "enumeration_complete": False},
            "distinct": {"kind": "$p", "status": "proved"},
            "same": {"kind": "$p", "status": "proved"},
        }
        mm.summarize_bundles(records)
        self.assertEqual(records["family"]["status"], "unbundling_limited")
        records["family"]["enumeration_complete"] = True
        records["same"]["status"] = "check_failed"
        mm.summarize_bundles(records)
        self.assertEqual(records["family"]["status"], "translation_failed")
        records["same"]["status"] = "proved"
        mm.summarize_bundles(records)
        self.assertEqual(records["family"]["status"], "proved")

    def test_primitive_bundles_preserve_assumptions_and_reject_forbidden_merges(self):
        db = mm.parse_database(PROPOSITIONAL + r"""
            $c setvar A. $. $v x y $. vx $f setvar x $. vy $f setvar y $.
            al $a wff A. x ph $.
            ${ h $e setvar y $. $d ph x $. ax $a |- A. x ph $. $}
        """)
        _, pieces, records = mm.generate_all(db, "test")
        variants = records["ax"]["variants"]
        self.assertEqual(len(variants), 2)
        self.assertTrue(all("(arr " + name in pieces[name] for name in variants))
        self.assertTrue(all(records[name]["kind"] == "$a" for name in variants))
        self.assertTrue(all("(dv " in pieces[name] for name in variants))
        apart = mm.replace(db["ax"], label="apart", dv=db["ax"].dv | {("x", "y")})
        with self.assertRaisesRegex(mm.TranslationError, "identification violates"):
            mm.specialize_statement(db, apart, ["x"], (0, 0))

    @unittest.skipUnless(shutil.which("cargo"), "Metacat end-to-end check requires Cargo")
    def test_exdistr_unbundles_and_both_cases_pass_metacat(self):
        # Use the actual exdistr proof, with its two dependencies as assumptions
        # so this regression stays independent of the large local set.mm file.
        db = mm.parse_database(r"""
            $c wff setvar |- ( ) /\ <-> E. $.
            $v ph ps x y $.
            wph $f wff ph $. wps $f wff ps $.
            vx $f setvar x $. vy $f setvar y $.
            wa $a wff ( ph /\ ps ) $.
            wex $a wff E. x ph $.
            wb $a wff ( ph <-> ps ) $.
            ${ $d x ph $.
               19.42v $a |- ( E. x ( ph /\ ps ) <-> ( ph /\ E. x ps ) ) $.
            $}
            ${ exbii.1 $e |- ( ph <-> ps ) $.
               exbii $a |- ( E. x ph <-> E. x ps ) $.
            $}
            ${ $d y ph $.
               exdistr $p |- ( E. x E. y ( ph /\ ps ) <-> E. x ( ph /\ E. y ps ) ) $=
                  ( wa wex 19.42v exbii ) ABEDFABDFECABDGH $.
            $}
        """)
        prefix, pieces, records = mm.generate_all(db, "test")
        variants = records["exdistr"]["variants"]
        self.assertEqual(variants, ["mm-unbundled-exdistr-0_1", "mm-unbundled-exdistr-0_0"])
        self.assertEqual(records[variants[0]]["additional_dv_pairs"], [("x", "y")])
        self.assertEqual(records[variants[1]]["identifications"], {"x": "x", "y": "x"})
        self.assertIn("E. x E. x", pieces[variants[1]])
        with tempfile.TemporaryDirectory(prefix="metacat-unbundling-test-") as temporary:
            path = Path(temporary) / "test.hex"
            mm.write_hex(path, prefix, pieces)
            retained, failures = mm.check_candidates(path, records, pieces)
            self.assertFalse(failures)
            self.assertEqual(set(retained), set(pieces))
            # The diagonal cannot be obtained by simply aliasing the two
            # inputs of the distinct case: it needs its own specialized proof.
            negative = dict(pieces)
            head = negative[variants[1]].split("\n    = ")[0]
            negative[variants[1]] = head + (
                "\n    = {[h0 h1 h2.] ([.h2] mm-copy-setvar [x y.]) "
                "([.h0 h1 x y] " + variants[0] + " [out.]) [.out]})\n")
            negative_records = {name: dict(record) for name, record in records.items()}
            negative_records[variants[1]]["dependencies"] = [variants[0]]
            mm.write_hex(path, prefix, negative)
            _, rejected = mm.check_candidates(path, negative_records, negative)
            self.assertEqual(set(rejected), {variants[1]})
            self.assertIn("forbidden reachability", rejected[variants[1]])
        mm.summarize_bundles(records)
        self.assertEqual(records["exdistr"]["status"], "proved")

    def test_failed_proofs_and_their_dependents_are_not_emitted(self):
        records = {
            "ax": {"status": "axiom", "dependencies": []},
            "bad": {"status": "check_failed", "dependencies": ["ax"]},
            "child": {"status": "proved", "dependencies": ["bad"]},
            "grandchild": {"status": "proved", "dependencies": ["child"]},
            "good": {"status": "proved", "dependencies": ["ax"]},
        }
        retained = mm.retain_checked(records, {name: name for name in records})
        self.assertEqual(list(retained), ["ax", "good"])
        self.assertEqual(records["grandchild"]["status"], "dependency_failed")

    def test_rejects_includes_and_unclosed_statements(self):
        for text in ("$[ other.mm $]", "${", "$c wff", "$( unclosed"):
            with self.subTest(text=text), self.assertRaises(mm.TranslationError):
                mm.parse_database(text)

    def test_wire_name_escaping_is_injective(self):
        names = ["x", "A", ".+", "mmv-2e2b"]
        self.assertEqual(len({mm.wire_name(v) for v in names}), len(names))


if __name__ == "__main__":
    unittest.main()
