#!/usr/bin/env python3
"""Translate a flattened Metamath set.mm database into Metacat Hex.

Usage (Python 3.10+, standard library only)::

    python3 translate_metamath.py set.mm -o examples/set.hex --check
    cargo run -- check examples/set.hex

The input must be self-contained: $[ includes $] are not supported. Syntax is
derived from syntactic $a declarations. Compressed and uncompressed proofs are
replayed (including substitutions and $d checks), then emitted as shared DAGs.

--check uses the repository's Rust verify_metamath example and retains only
successfully checked proofs with checked dependencies. Without it, the output
contains candidates, not Metacat-verified proofs. In either mode a JSON report
and a TSV list of untranslated theorems accompany the output. Failed theorems
are never introduced as axioms. Unavailable lemmas are inlined where possible.

Bundled Metamath theorems are unbundled into explicit identification patterns
compatible with their $d conditions. A merged case uses one metavariable in
several places; distinct bare variables remain distinct. Proof applications
select the matching variant. Ordinary non-bare metavariables need no splitting.
Derived interfaces may omit redundant $d pairs, but --check verifies the
resulting stronger theorem; primitive assumptions are never weakened.

Unbundling can grow exponentially. --max-variants limits eager enumeration per
assertion (0 is unlimited); additional variants required by proofs are generated
on demand. Incomplete families and other failures are reported, never counted
as full ports. --max-steps separately limits expanded proof size.

--mini retains the small foundational/sp fragment and its readable syntax in
examples/mini-set.hex, using the same proof-based bareness inference as the full
translation.
"""
import argparse
import hashlib
import itertools
import json
import re
import subprocess
import sys
import tempfile
import time
from collections import defaultdict, deque
from dataclasses import dataclass, replace
from pathlib import Path
from functools import lru_cache


class TranslationError(Exception):
    """Malformed input, an invalid proof, or an unsupported translation."""


def require(condition, message):
    """Keep validation enabled even when Python runs with -O."""
    if not condition:
        raise TranslationError(message)


@dataclass
class Statement:
    label: str
    kind: str
    expression: tuple
    hypotheses: tuple = ()
    dv: frozenset = frozenset()
    context_dv: frozenset = frozenset()
    proof: tuple = ()
    position: int = 0
    scope_end: int | None = None
    context_variables: frozenset = frozenset()


def read_database(path):
    return parse_database(Path(path).read_text(encoding="utf-8"))


def parse_database(text):
    """Collect assertions with their mandatory hypotheses and scoped $d pairs."""
    text = re.sub(r"\$\(.*?\$\)", " ", text, flags=re.S)
    require("$(" not in text and "$)" not in text, "unbalanced Metamath comment")
    tokens = iter(text.split())

    def until(end):
        body = []
        for token in tokens:
            if token == end:
                return tuple(body)
            require(not token.startswith("$"), f"expected {end}, found {token}")
            body.append(token)
        raise TranslationError(f"unexpected end of input; expected {end}")

    statements = {}
    variables = frozenset()
    hypotheses = []
    disjoint = set()
    scopes = []
    for token in tokens:
        if token == "${":
            scopes.append((variables, hypotheses.copy(), disjoint.copy()))
        elif token == "$}":
            require(scopes, "unmatched $}")
            for label in hypotheses[len(scopes[-1][1]):]:
                statements[label].scope_end = len(statements)
            variables, hypotheses, disjoint = scopes.pop()
        elif token == "$[":
            raise TranslationError("$[ includes $] are not supported; supply a flattened database")
        elif token in ("$c", "$v", "$d"):
            body = until("$.")
            if token == "$v":
                variables = variables.union(body)
            elif token == "$d":
                require(len(body) >= 2 and len(set(body)) == len(body), "invalid $d declaration")
                require(set(body) <= variables, "$d uses an undeclared variable")
                disjoint.update(tuple(sorted(pair)) for pair in itertools.combinations(body, 2))
        else:
            kind = next(tokens, None)
            require(kind in ("$f", "$e", "$a", "$p"), f"{token}: unsupported statement kind {kind}")
            body = until("$=" if kind == "$p" else "$.")
            require(body, f"{token}: empty expression")
            proof = until("$.") if kind == "$p" else ()
            require(token not in statements, f"duplicate label {token}")
            if kind == "$f":
                require(len(body) == 2 and body[1] in variables, f"{token}: invalid floating hypothesis")
            statement = Statement(token, kind, body, proof=proof, position=len(statements))
            if kind in ("$f", "$e"):
                hypotheses.append(token)
            else:
                mandatory = set(body) & variables
                for label in hypotheses:
                    hypothesis = statements[label]
                    if hypothesis.kind == "$e":
                        mandatory.update(set(hypothesis.expression) & variables)
                statement.hypotheses = tuple(
                    label for label in hypotheses
                    if statements[label].kind == "$e" or statements[label].expression[1] in mandatory
                )
                statement.dv = frozenset(pair for pair in disjoint if set(pair) <= mandatory)
                statement.context_dv = frozenset(disjoint)
                statement.context_variables = variables
                typed = [statements[h].expression[1] for h in statement.hypotheses if statements[h].kind == "$f"]
                require(len(typed) == len(set(typed)) and set(typed) == mandatory,
                        f"{token}: mandatory variables must each have one active $f hypothesis")
            statements[token] = statement
    require(not scopes, "unclosed ${ scope")
    return statements


def references(statement):
    proof = statement.proof
    if not proof:
        return ()
    if proof[0] == "(":
        require(")" in proof, f"{statement.label}: unterminated compressed proof label list")
        return proof[1:proof.index(")")]
    return proof


def dependencies(database, target):
    selected = set()
    active = set()
    def visit(label):
        if label in selected:
            return
        require(label in database, f"unknown proof label {label}")
        require(label not in active, f"cyclic proof dependency at {label}")
        active.add(label)
        for dependency in references(database[label]):
            visit(dependency)
        active.remove(label)
        selected.add(label)
    visit(target)
    return [statement for label, statement in database.items() if label in selected]


@dataclass(eq=False)
class Step:
    label: str
    arguments: tuple
    expression: tuple
    local: bool = False


def prove(database, theorem):
    """Decode and check a Metamath proof, retaining its shared proof DAG."""
    stack = []
    saved = []
    hypotheses = {}
    variables = theorem.context_variables
    require(theorem.proof, f"{theorem.label}: missing proof")
    def apply(label):
        require(label in database, f"{theorem.label}: unknown proof label {label}")
        statement = database[label]
        require(statement.position < theorem.position,
                f"{theorem.label}: forward or circular reference to {label}")
        if statement.kind in ("$f", "$e"):
            require(statement.scope_end is None or theorem.position < statement.scope_end,
                    f"{theorem.label}: hypothesis {label} is out of scope")
            node = hypotheses.setdefault(label, Step(label, (), statement.expression))
            stack.append(node)
            return
        count = len(statement.hypotheses)
        require(len(stack) >= count, f"{theorem.label}: stack underflow applying {label}")
        arguments = tuple(stack[-count:]) if count else ()
        if count:
            del stack[-count:]
        substitution = {}
        for hypothesis, argument in zip(statement.hypotheses, arguments):
            h = database[hypothesis]
            if h.kind == "$f":
                require(h.expression[0] == argument.expression[0],
                        f"{theorem.label}: {label} has the wrong type for {hypothesis}")
                substitution[h.expression[1]] = argument.expression[1:]
        subst = lambda expr: tuple(y for x in expr for y in substitution.get(x, (x,)))
        for hypothesis, argument in zip(statement.hypotheses, arguments):
            h = database[hypothesis]
            if h.kind == "$e":
                require(subst(h.expression) == argument.expression,
                        f"{theorem.label}: {label} does not satisfy hypothesis {hypothesis}")
        for left, right in statement.dv:
            for x in set(substitution[left]) & variables:
                for y in set(substitution[right]) & variables:
                    require(x != y and tuple(sorted((x, y))) in theorem.context_dv,
                            f"{theorem.label}: {label} requires $d {x} {y}")
        node = Step(label, arguments, subst(statement.expression))
        stack.append(node)
    if theorem.proof[0] != "(":
        for label in theorem.proof:
            apply(label)
    else:
        require(")" in theorem.proof, f"{theorem.label}: unterminated compressed proof label list")
        end = theorem.proof.index(")")
        labels = theorem.hypotheses + theorem.proof[1:end]
        for label in labels:
            require(label in database and database[label].position < theorem.position,
                    f"{theorem.label}: invalid compressed proof label {label}")
        value = 0
        for char in "".join(theorem.proof[end + 1:]):
            if "U" <= char <= "Y":
                value = value * 5 + ord(char) - ord("U") + 1
            elif "A" <= char <= "T":
                value = value * 20 + ord(char) - ord("A") + 1
                if value <= len(labels):
                    apply(labels[value - 1])
                else:
                    index = value - len(labels) - 1
                    require(index < len(saved), f"{theorem.label}: invalid saved proof index {index}")
                    stack.append(saved[index])
                value = 0
            else:
                require(char == "Z" and stack and value == 0,
                        f"{theorem.label}: invalid or incomplete compressed proof at {char!r}")
                saved.append(stack[-1])
        require(value == 0, f"{theorem.label}: unfinished compressed proof number")
    require(len(stack) == 1 and stack[0].expression == theorem.expression,
            f"{theorem.label}: proof does not establish its stated conclusion")
    return stack[0]


class FormulaParser:
    """The first-order fragment used by the foundational axioms and sp."""
    def __init__(self, variables):
        self.variables = variables
        self.constructors = {}

    def node(self, name, *args):
        require(self.constructors.setdefault(name, len(args)) == len(args),
                f"inconsistent constructor arity for {name}")
        return (name, *args)

    def parse(self, expression):
        try:
            return self._parse(expression)
        except (KeyError, IndexError, ValueError) as error:
            raise TranslationError("unsupported fragment syntax: " + " ".join(expression)) from error

    def _parse(self, expression):
        tokens = expression[1:]
        sort = expression[0]
        @lru_cache(None)
        def term(sort, i):
            token = tokens[i]
            if token in self.variables and self.variables[token] == sort:
                return token, i + 1
            if sort == "class" and self.variables.get(token) == "setvar":
                return self.node("cv", token), i + 1
            if sort == "class":
                if token in ("(/)", "_om", "~~"):
                    return self.node({"(/)": "empty", "_om": "omega", "~~": "equinumerosity"}[token]), i + 1
                if token in ("dom", "ran", "suc"):
                    value, end = term("class", i + 1)
                    return self.node(token, value), end
                if token == "(":
                    function, j = term("class", i + 1)
                    require(tokens[j] == "`", "expected function application")
                    argument, end = term("class", j + 1)
                    require(tokens[end] == ")", "expected closing parenthesis")
                    return self.node("apply", function, argument), end + 1
            if sort == "wff":
                if token == "-.":
                    value, end = term("wff", i + 1)
                    return self.node("-.", value), end
                if token in ("A.", "E."):
                    variable, j = term("setvar", i + 1)
                    if tokens[j] == "e.":
                        domain, j = term("class", j + 1)
                        body, end = term("wff", j)
                        require(token == "A.", "restricted existential is outside the mini fragment")
                        return self.node("rforall", variable, domain, body), end
                    body, end = term("wff", j)
                    return self.node("forall" if token == "A." else "exists", variable, body), end
                if token == "(":
                    try:
                        left, j = term("wff", i + 1)
                        operator = {"->": "=>", "<->": "iff", "/\\": "and", "\\/": "or"}[tokens[j]]
                        right, end = term("wff", j + 1)
                        require(tokens[end] == ")", "expected closing parenthesis")
                        return self.node(operator, left, right), end + 1
                    except (ValueError, KeyError, TranslationError):
                        pass  # A function application can begin a class relation.
                left, j = term("class", i)
                operator = {"=": "=", "e.": "in", "=/=": "ne", "C_": "subset"}.get(tokens[j])
                if operator is None:
                    relation, j = term("class", j)
                    right, end = term("class", j)
                    return self.node("rel", left, relation, right), end
                right, end = term("class", j + 1)
                return self.node(operator, left, right), end
            raise ValueError((sort, i, tokens))
        tree, end = term("wff" if sort == "|-" else sort, 0)
        require(end == len(tokens), "trailing tokens in expression: " + " ".join(expression))
        return tree


class SyntaxGrammar:
    """Parse formulas using the input's $a syntax rules, represented as tries.

    Earley states share common rule prefixes. Leaves are floating hypotheses;
    each completed rule becomes one constructor, ordered by its $f hypotheses.
    Equivalent productions use the first declaration in database order.
    """

    def __init__(self, database):
        self.nodes = []
        self.roots = {}
        self.constructors = {}
        self.variables = {}
        self.cache = {}

        def new_node():
            self.nodes.append(({}, []))
            return len(self.nodes) - 1

        for statement in database.values():
            if statement.kind != "$a" or statement.expression[0] == "|-":
                continue
            hs = floats(database, statement)
            require(len(hs) == len(statement.hypotheses),
                    f"{statement.label}: syntax rules with essential hypotheses are unsupported")
            types = {h.expression[1]: h.expression[0] for h in hs}
            rhs = statement.expression[1:]
            require(rhs and all(rhs.count(v) == 1 for v in types),
                    f"{statement.label}: nonlinear or empty syntax production")
            sort = statement.expression[0]
            if sort not in self.roots:
                self.roots[sort] = new_node()
            node = self.roots[sort]
            order = []
            for token in rhs:
                symbol = (types[token],) if token in types else token
                if token in types:
                    order.append(token)
                edges = self.nodes[node][0]
                if symbol not in edges:
                    edges[symbol] = new_node()
                node = edges[symbol]
            name = "mm-" + statement.label
            positions = tuple(order.index(h.expression[1]) for h in hs)
            self.nodes[node][1].append((sort, name, positions))
            self.constructors[name] = len(hs)

    def parse(self, expression):
        sort = "wff" if expression[0] == "|-" else expression[0]
        tokens = expression[1:]
        variables = {v: self.variables[v] for v in set(tokens) & self.variables.keys()}
        key = (sort, tokens, tuple(sorted(variables.items())))
        if key in self.cache:
            return self.cache[key]
        n = len(tokens)
        charts = [dict() for _ in range(n + 1)]
        agendas = [deque() for _ in range(n + 1)]
        waiting = [defaultdict(list) for _ in range(n + 1)]
        completed = {}

        def add(end, node, start, children):
            state = (node, start)
            if state not in charts[end]:
                charts[end][state] = children
                agendas[end].append(state)

        require(tokens, f"empty {sort} expression")
        if len(tokens) == 1 and variables.get(tokens[0]) == sort:
            return tokens[0]
        require(sort in self.roots, f"no syntax grammar for {sort}")
        add(0, self.roots[sort], 0, ())
        answer = None
        for end in range(n + 1):
            while agendas[end]:
                node, start = agendas[end].popleft()
                children = charts[end][(node, start)]
                edges, results = self.nodes[node]
                for result_sort, name, positions in results:
                    done = (result_sort, start, end)
                    if done in completed:
                        continue
                    tree = (name, *(children[p] for p in positions))
                    completed[done] = tree
                    if result_sort == sort and start == 0 and end == n:
                        answer = tree
                    for successor, origin, previous in waiting[start][result_sort]:
                        add(end, successor, origin, previous + (tree,))
                if end == n:
                    continue
                token = tokens[end]
                if token in edges:
                    add(end + 1, edges[token], start, children)
                for child_sort in ("setvar", "class", "wff"):
                    successor = edges.get((child_sort,))
                    if successor is None:
                        continue
                    waiting[end][child_sort].append((successor, start, children))
                    if variables.get(token) == child_sort:
                        add(end + 1, successor, start, children + (token,))
                    if child_sort in self.roots:
                        add(end, self.roots[child_sort], end, ())
        require(answer is not None, "cannot parse expression: " + " ".join(expression))
        if len(self.cache) >= 20000:
            self.cache.clear()
        self.cache[key] = answer
        return answer


def floats(database, statement):
    return [database[label] for label in statement.hypotheses if database[label].kind == "$f"]


def mandatory_variables(database, statement):
    return [h.expression[1] for h in floats(database, statement)]


def tensor(items):
    return "{" + " ".join(items) + "}"


def wire_name(name):
    """Escape Metamath metavariable names outside hexpr's wire alphabet."""
    if re.fullmatch(r"[A-Za-z0-9_-]+", name) and not name.startswith("mmv-"):
        return name
    return "mmv-" + name.encode("utf-8").hex()


def render(tree):
    if isinstance(tree, str):
        return "[." + wire_name(tree) + "]"
    if len(tree) == 1:
        return tree[0]
    children = tree[1:]
    if all(isinstance(child, str) for child in children):
        inputs = "[." + " ".join(map(wire_name, children)) + "]"
    elif len(children) == 1:
        inputs = render(children[0])
    else:
        inputs = tensor([render(child) for child in children])
    return "(" + inputs + " " + tree[0] + ")"


def ar_annotation(database, statement, bare):
    if not bare:
        return ""
    variables = mandatory_variables(database, statement)
    formulae = [v for v in variables if v not in bare]
    if not formulae:
        return " (dv " + ("del" if len(bare) == 1 else tensor(["del"] * len(bare))) + ")"
    if len(bare) == len(formulae) == 1:
        return " (dv " + ("(del sel)" if statement.dv else "_") + ")"
    permitted = lambda x, y: tuple(sorted((x, y))) not in statement.dv
    if len(formulae) == 1:
        allowed = [permitted(variable, formulae[0]) for variable in bare]
        rows = tensor(["_" if p else "del" for p in allowed])
        def merge(n):
            return "sel" if n == 0 else "_" if n == 1 else "sup" if n == 2 else "(" + tensor(["_", merge(n - 1)]) + " sup)"
        return " (dv (" + rows + " " + merge(sum(allowed)) + "))"
    if not any(permitted(x, y) for x in bare for y in formulae):
        return " (dv (" + tensor(["del"] * len(bare)) + " " + tensor(["sel"] * len(formulae)) + "))"
    # Shared wire names encode the relation directly: each source is copied
    # to its permitted target places and targets merge incoming copies.
    rows = []
    edges = []
    for i, variable in enumerate(bare):
        labels = []
        for j, other in enumerate(formulae):
            if tuple(sorted((variable, other))) not in statement.dv:
                label = "r" + str(i) + "c" + str(j)
                edges.append((label, j))
                labels.append(label)
        # dup/del is represented by a single spider, with the source named b_i.
        if not labels:
            rows.append("del")
        elif len(labels) == 1:
            rows.append("[" + labels[0] + ".]")
        else:
            # Use explicit dup recursively to keep outputs separate until the
            # per-target merge stage; naming them routes the resulting wires.
            def duplicate(n):
                return "_" if n == 1 else "(dup " + tensor(["_", duplicate(n - 1)]) + ")"
            rows.append("(" + duplicate(len(labels)) + " [" + " ".join(labels) + ".])")
    outputs = []
    for j, other in enumerate(formulae):
        labels = [label for label, target in edges if target == j]
        def merge(n):
            return "sel" if n == 0 else "_" if n == 1 else "(" + tensor(["_", merge(n - 1)]) + " sup)"
        outputs.append("([." + " ".join(labels) + "] " + merge(len(labels)) + ")")
    # All inputs are consumed in the first tensor; outputs are supplied by
    # the second tensor, with the shared names connecting the two halves.
    return " (dv (" + tensor(rows) + " " + tensor(outputs) + "))"


ALIASES = {"ax7v": "ax-7", "ax7v1": "ax-7", "ax7v2": "ax-7"}


def proof_hex(database, statement, root, aliases=None):
    aliases = ALIASES if aliases is None else aliases
    names = {label: "h" + str(i) for i, label in enumerate(statement.hypotheses)}
    nodes = {}
    applications = {}
    commands = []
    def visit(step):
        if step in nodes:
            return nodes[step]
        s = database[step.label]
        if s.kind in ("$f", "$e"):
            if step.local or step.label not in names:
                require(s.kind == "$f" and s.expression[0] in ("setvar", "wff", "class"),
                        f"{statement.label}: cannot introduce local hypothesis {step.label}")
                name = "dummy-" + str(len(nodes))
                operation = "fresh" if s.expression[0] == "setvar" else "fresh-" + s.expression[0]
                commands.append("(" + operation + " [" + name + ".])")
                nodes[step] = name
                if not step.local:
                    names[step.label] = name
                return name
            return names[step.label]
        args = [visit(argument) for argument in step.arguments]
        key = (step.label, tuple(args))
        if key in applications:
            nodes[step] = applications[key]
            return nodes[step]
        name = "p" + str(len(commands))
        nodes[step] = name
        applications[key] = name
        commands.append("([." + " ".join(args) + "] " + aliases.get(step.label, step.label) + " [" + name + ".])")
        return name
    output = visit(root)
    inputs = " ".join(names[label] for label in statement.hypotheses)
    return "{[" + inputs + ".]\n    " + "\n    ".join(commands) + "\n    [." + output + "]}"


def declaration(database, parser, statement, bare, proof=None, aliases=None):
    parser.variables = {h.expression[1]: h.expression[0] for h in floats(database, statement)}
    variables = mandatory_variables(database, statement)
    variables = bare + [v for v in variables if v not in bare]
    def typed(expression):
        return "(" + render(parser.parse(expression)) + " " + expression[0] + ")"
    source = "([" + " ".join(map(wire_name, variables)) + ".] " + tensor([typed(database[h].expression) for h in statement.hypotheses]) + ")"
    target = "([" + " ".join(map(wire_name, variables)) + ".] " + typed(statement.expression) + ")"
    prefix = "def" if proof else "arr"
    annotation = ar_annotation(database, statement, bare)
    text = "  # " + statement.label + ": " + " ".join(statement.expression) + "\n"
    if statement.dv:
        text += "  # $d " + ", ".join(" ".join(pair) for pair in sorted(statement.dv)) + "\n"
    text += "  (" + prefix + " " + statement.label + annotation + "\n    : " + source + "\n    -> " + target
    if proof:
        text += "\n    = " + proof_hex(database, statement, proof, aliases)
    return text + ")\n"


def generate(database, source_hash):
    """Translate the small foundational/sp fragment with inferred DV interfaces."""
    for label, replacement in ALIASES.items():
        require(label in database and replacement in database, f"missing fragment alias {label}")
        original, base = database[label], database[replacement]
        prove(database, original)
        require(original.expression == base.expression and original.hypotheses == base.hypotheses
                and base.dv <= original.dv, f"{label}: fragment alias adaptation no longer applies")
    selected = {s.label for s in dependencies(database, "sp") if s.kind in ("$a", "$p") and s.label not in ALIASES}
    # ax-groth begins the extension beyond ZFC; later axioms belong to other
    # theories and mathboxes rather than the requested foundational fragment.
    for s in database.values():
        if s.label == "ax-groth":
            break
        if s.kind == "$a" and s.label.startswith("ax-"):
            selected.add(s.label)
    selected.update(("wa", "wo", "wcel", "df-an", "df-or"))
    for label in ("wne", "wral", "wss", "wbr", "c0", "com", "cen", "cdm", "crn", "csuc", "cfv"):
        selected.add(label)
    parser = FormulaParser({s.expression[1]: s.expression[0] for s in database.values() if s.kind == "$f"})
    translator = ProofTranslator(database, max_steps=20000)
    output = []
    for s in database.values():
        if s.label not in selected:
            continue
        proof = prove(database, s) if s.kind == "$p" else None
        bare = translator.requirements(s, proof)
        setvars = [h.expression[1] for h in floats(database, s) if h.expression[0] == "setvar"]
        require(all(x == y or tuple(sorted((x, y))) in s.dv for x in bare for y in setvars),
                f"{s.label}: this fragment needs unbundling; use the full translation")
        require(s.kind == "$p" or all(set(pair) & set(bare) for pair in s.dv),
                f"{s.label}: axiom DV pattern is not representable")
        if proof:
            proof, _ = translator.expand(s, proof)
        output.append(declaration(database, parser, s, bare, proof, aliases={}))
        translator.bare[s.label] = bare
    syntax = ["(theory set.syntax nat {", "  (arr wff : 1 -> 1)", "  (arr setvar : 1 -> 1)", "  (arr class : 1 -> 1)", "  (arr |- : 1 -> 1)"]
    syntax.extend("  (arr " + name + " : " + str(arity) + " -> 1)" for name, arity in parser.constructors.items())
    syntax.append("})")
    header = f"""# Foundational logic/ZFC axioms and the proof of sp from Metamath's set.mm.
# Source: local set.mm (CC0, https://github.com/metamath/set.mm).
# SHA-256: {source_hash}
# Regenerate: python3 scripts/translate_metamath.py set.mm --mini
# Check every proof: cargo run -- check examples/mini-set.hex
#
# Scope: the 29 foundational ax-* declarations through ax-ac2, including
# redundant variants and weaker choice axioms; no ax-groth or mathbox axioms.
# Theorems are the dependency chain of sp, with the original proof DAGs.
# Definitions are df-bi, df-or, df-an and df-ex. Other notation appearing only
# in ax-cc/ax-dc is declared syntactically; its definitions are not ported here.
#
# wff/setvar/class are Metamath's floating hypotheses, retained as proof inputs;
# |- marks provability. cv embeds a set variable into class syntax. Names of
# connectives are adapted to hexpr: =>, iff, and, or, forall, exists, = and in.
# Each declaration's comment gives the original Metamath statement and $d pairs.
# h0, h1, ... are its hypotheses in Metamath order; p0, p1, ... are proof steps.
#
# DV annotations order syntax metavariables as B + C. An ar edge permits
# occurrence of a bare variable (B) in an ordinary metavariable (C). Derived
# interfaces use the full translator's proof-based bareness inference; redundant
# source $d conditions need not become annotations. sp needs no DV annotation.
# ax7v/ax7v1/ax7v2 are inlined to ax-7, preserving the original fragment's scope.
# fresh introduces a local set-variable hypothesis, not a provability axiom;
# its uses in equid and 19.8a discharge the original dummy-variable conditions.

"""
    return header + "\n".join(syntax) + "\n\n(theory set.proof set.syntax {\n  # A local set-variable declaration, used for Metamath's dummy variables.\n  (arr fresh (dv del) : [x.] -> ([x] setvar))\n\n" + "\n".join(output) + "})\n"


def proof_steps(root):
    """Visit a shared proof DAG once, with premises before applications."""
    seen = set()
    agenda = [(root, False)]
    while agenda:
        step, ready = agenda.pop()
        if step in seen:
            continue
        if ready:
            seen.add(step)
            yield step
        else:
            agenda.append((step, True))
            agenda.extend((child, False) for child in reversed(step.arguments))


def unbundled_patterns(variables, bare, disjoint):
    """Enumerate B-containing equivalence classes; leave C-only sharing implicit.

    Every class must be independent of the source's $d relation. Choosing the
    first remaining bare variable makes each relevant partition occur once.
    The all-distinct pattern is first, and representatives use source order.
    """
    n = len(variables)
    required = {i for i, v in enumerate(variables) if v in bare}
    forbidden = {i: {j for j, w in enumerate(variables)
                     if tuple(sorted((v, w))) in disjoint}
                 for i, v in enumerate(variables)}

    def visit(remaining, pattern):
        pending = remaining & required
        if not pending:
            yield tuple(pattern)
            return
        first = min(pending)
        candidates = sorted(remaining - {first} - forbidden[first])

        def groups(offset, group):
            if offset == len(candidates):
                yield group
                return
            other = candidates[offset]
            yield from groups(offset + 1, group)
            if not (forbidden[other] & group):
                yield from groups(offset + 1, group | {other})

        for group in groups(0, {first}):
            copied = pattern.copy()
            for i in group:
                copied[i] = min(group)
            yield from visit(remaining - group, copied)

    yield from visit(set(range(n)), list(range(n)))


def application_pattern(variables, bare, substitutions):
    """Identify actual set variables only in classes containing a bare formal."""
    actuals = [substitutions[v] for v in variables]
    required = {substitutions[v] for v in bare}
    return tuple(actuals.index(actual) if actual in required else i
                 for i, actual in enumerate(actuals))


@dataclass
class Variant:
    statement: Statement
    # Each variant input comes from this position in the source application.
    inputs: tuple
    renaming: dict
    hypotheses: dict
    bare: list


def specialize_statement(database, source, bare, pattern):
    """Quotient set metavariables and hypotheses, preserving every source $d pair."""
    setvars = [h.expression[1] for h in floats(database, source) if h.expression[0] == "setvar"]
    renaming = {v: setvars[pattern[i]] for i, v in enumerate(setvars)}
    name = "mm-unbundled-" + source.label + "-" + "_".join(map(str, pattern))
    require(name not in database, f"reserved variant name already exists: {name}")
    rename = lambda expr: tuple(renaming.get(x, x) for x in expr)
    mapped_dv = frozenset(tuple(sorted(rename(pair))) for pair in source.dv)
    require(all(x != y for x, y in mapped_dv), f"{source.label}: identification violates $d")
    mapped_bare = list(dict.fromkeys(renaming[v] for v in bare))
    representatives = set(renaming.values())
    extra = frozenset(tuple(sorted((x, y))) for x in mapped_bare for y in representatives if x != y)
    inputs, hypotheses, bindings, floating = [], [], {}, {}
    for i, label in enumerate(source.hypotheses):
        h = database[label]
        expression = rename(h.expression)
        if h.kind == "$f" and expression in floating:
            bindings[label] = floating[expression]
            continue
        renamed = f"{name}-h{i}"
        require(renamed not in database, f"reserved hypothesis name already exists: {renamed}")
        database[renamed] = replace(h, label=renamed, expression=expression)
        if h.kind == "$f":
            floating[expression] = renamed
        bindings[label] = renamed
        hypotheses.append(renamed)
        inputs.append(i)
    statement = replace(source, label=name, expression=rename(source.expression),
                        hypotheses=tuple(hypotheses), dv=mapped_dv | extra,
                        context_dv=frozenset(tuple(sorted(rename(p))) for p in source.context_dv) | extra,
                        context_variables=frozenset(rename(source.context_variables)))
    database[name] = statement
    return Variant(statement, tuple(inputs), renaming, bindings, mapped_bare)


def specialize_proof(root, variant):
    """Apply an identification to the replayed DAG, sharing collapsed $f inputs."""
    copies, hypotheses = {}, {}
    for step in proof_steps(root):
        expression = tuple(variant.renaming.get(x, x) for x in step.expression)
        if step.label in variant.hypotheses:
            label = variant.hypotheses[step.label]
            copied = hypotheses.setdefault(label, Step(label, (), expression))
        else:
            copied = Step(step.label, tuple(copies[c] for c in step.arguments), expression, step.local)
        copies[step] = copied
    return copies[root]


class ProofTranslator:
    """Replay source proofs and inline lemmas unavailable as checked generators."""

    def __init__(self, database, max_steps):
        self.database = dict(database)
        self.bare = {}
        self.required_bare = {}
        self.variants = {}
        self.ensure_variant = None
        self.max_steps = max_steps
        self.template = lru_cache(maxsize=64)(lambda label: prove(database, database[label]))

    def expand(self, statement, root):
        memo = {}
        applications = {}
        count = 0
        fresh = 0
        inlined = set()
        boundary_variables = set(mandatory_variables(self.database, statement))

        def rewrite(step):
            nonlocal count, fresh
            if step in memo:
                return memo[step]
            s = self.database[step.label]
            if s.kind in ("$f", "$e"):
                return step
            args = tuple(rewrite(child) for child in step.arguments)
            key = (step.label, args)
            if key in applications:
                memo[step] = applications[key]
                return memo[step]
            count += 1
            require(not self.max_steps or count <= self.max_steps,
                    f"expanded proof exceeds {self.max_steps} steps")
            selected, selected_args = step.label, args
            if selected in self.variants:
                substitutions = {
                    self.database[h].expression[1]: arg.expression[1:]
                    for h, arg in zip(s.hypotheses, args) if self.database[h].kind == "$f"
                }
                setvars = [h.expression[1] for h in floats(self.database, s) if h.expression[0] == "setvar"]
                pattern = application_pattern(setvars, self.required_bare[selected], substitutions)
                family = self.variants[selected]
                if pattern not in family and self.ensure_variant is not None:
                    self.ensure_variant(s, pattern)
                variant = family.get(pattern)
                # Distinct classes must really be distinct in this caller.
                # Local metavariables can be chosen fresh; boundary pairs need
                # the caller's explicit assumptions. Coinciding inputs instead
                # select a merged variant, never the distinct-variable case.
                def permitted_pair(pair):
                    left, right = (substitutions[v] for v in pair)
                    if len(left) != 1 or len(right) != 1 or left == right:
                        return False
                    if left[0] in boundary_variables and right[0] in boundary_variables:
                        return tuple(sorted((left[0], right[0]))) in statement.dv
                    return True

                if variant is not None:
                    mapped_dv = {tuple(sorted(variant.renaming.get(v, v) for v in pair)) for pair in s.dv}
                    if all(permitted_pair(pair) for pair in variant.statement.dv - mapped_dv):
                        selected = variant.statement.label
                        selected_args = tuple(args[i] for i in variant.inputs)
            if selected in self.bare:
                result = Step(selected, selected_args, step.expression)
            else:
                require(s.kind == "$p", f"unavailable axiom {s.label}")
                inlined.add(s.label)
                bindings = dict(zip(s.hypotheses, args))
                copies = {}

                def instantiate(node):
                    nonlocal fresh
                    if node in copies:
                        return copies[node]
                    original = self.database[node.label]
                    if node.label in bindings:
                        return bindings[node.label]
                    if original.kind == "$f":
                        require(original.expression[0] in ("setvar", "wff", "class"),
                                f"cannot inline {s.label}: local {original.expression[0]} hypothesis")
                        fresh += 1
                        copied = Step(node.label, (), (original.expression[0], f"@local{fresh}"), local=True)
                    else:
                        require(original.kind in ("$a", "$p"), "unbound essential hypothesis")
                        children = tuple(instantiate(child) for child in node.arguments)
                        substitution = {
                            self.database[h].expression[1]: child.expression[1:]
                            for h, child in zip(original.hypotheses, children)
                            if self.database[h].kind == "$f"
                        }
                        expression = tuple(y for x in original.expression for y in substitution.get(x, (x,)))
                        copied = Step(node.label, children, expression)
                    copies[node] = copied
                    return copied

                result = rewrite(instantiate(self.template(s.label)))
            memo[step] = result
            applications[key] = result
            return result

        return rewrite(root), sorted(inlined)

    def requirements(self, statement, root):
        """Propagate bareness through source proofs before expanding any lemmas."""
        variables = mandatory_variables(self.database, statement)
        setvars = [h.expression[1] for h in floats(self.database, statement) if h.expression[0] == "setvar"]
        # Primitive $d conditions must be encoded verbatim. For a derived
        # theorem, require only the bareness actually used by its proof; adding
        # redundant bare variables here can unnecessarily constrain callers.
        bare = {x for x in setvars if any(x in pair for pair in statement.dv)} if root is None else set()
        if root is None:
            eligible = {x for x in bare if all(x == y or tuple(sorted((x, y))) in statement.dv for y in setvars)}
            if all(set(pair) & eligible for pair in statement.dv):
                bare = eligible
            # Choose a smaller B when every original $d pair is still covered.
            # B/C already forbids identifying a B variable with a C variable;
            # both endpoints need not be B. Prefer a cover that needs no split;
            # otherwise unbundling accounts for the additional distinctness.
            for variable in variables:
                remaining = bare - {variable}
                if all(left in remaining or right in remaining for left, right in statement.dv):
                    bare = remaining
        if root:
            for step in proof_steps(root):
                if self.database[step.label].kind not in ("$a", "$p"):
                    continue
                called = self.database[step.label]
                for h, argument in zip(called.hypotheses, step.arguments):
                    hypothesis = self.database[h]
                    required = self.required_bare.get(step.label, self.bare.get(step.label, ()))
                    if hypothesis.kind != "$f" or hypothesis.expression[1] not in required:
                        continue
                    actual = argument.expression[1:]
                    require(len(actual) == 1, f"{step.label}: bare metavariable has compound syntax")
                    if actual[0] in variables:
                        bare.add(actual[0])
        result = [x for x in variables if x in bare]
        self.required_bare[statement.label] = result
        return result


def generate_all(database, source_hash, excluded=(), max_steps=20000, through=None, max_variants=128):
    """Build a candidate theory and an outcome for every selected assertion.

    Only original $a declarations/their specializations and local-variable
    introduction become axioms. Unavailable $p variants are inlined if possible.
    Redundant, unrepresentable $d pairs may disappear from derived interfaces,
    but these stronger claims must subsequently pass Metacat's DV checker.
    """
    grammar = SyntaxGrammar(database)
    translator = ProofTranslator(database, max_steps)
    statements = list(database.values())
    database = translator.database
    for name in ("fresh", "fresh-wff", "fresh-class"):
        require(name not in database, f"input label {name} conflicts with local-variable introduction")
    syntax = ["(theory set.syntax nat {"]
    syntax.extend(f"  (arr {sort} : 1 -> 1)" for sort in ("wff", "setvar", "class", "|-"))
    syntax.extend(f"  (arr {name} : {arity} -> 1)" for name, arity in grammar.constructors.items())
    syntax.append("})")
    header = (
        "# Generated from Metamath set.mm by translate_metamath.py.\n"
        f"# Source SHA-256: {source_hash}\n"
        "# Source license: CC0 (https://github.com/metamath/set.mm).\n"
        "# Includes original axioms/definitions, including mathbox assumptions.\n"
        "# See set.report.json and set-untranslated.tsv for coverage and failures.\n"
        "# Constructors mm-LABEL come from the source's syntactic $a declarations.\n"
        "# Floating and essential hypotheses are explicit proof inputs.\n"
        "# Some derived interfaces omit redundant $d pairs; retained proofs must\n"
        "# pass Metacat DV checking under those weaker assumptions.\n\n"
        "# mm-unbundled-LABEL-PATTERN explicitly specialize variable identifications.\n"
        "# A source theorem counts as ported only when its entire family is checked.\n\n"
    )
    prefix = header + "\n".join(syntax) + "\n\n(theory set.proof set.syntax {\n"
    for sort in ("setvar", "wff", "class"):
        name = "fresh" if sort == "setvar" else "fresh-" + sort
        prefix += f"  (arr {name} (dv del) : [x.] -> ([x] {sort}))\n"
    prefix += "\n"
    pieces = {}
    records = {}
    sources = {}
    started = time.monotonic()
    processed = 0

    def emit(s, root, bare, record):
        if root:
            root, inlined = translator.expand(s, root)
            record["inlined"] = inlined
        uncovered = [pair for pair in sorted(s.dv) if not (set(pair) & set(bare))]
        require(not uncovered or s.kind == "$p", "axiom DV pattern is not representable")
        piece = declaration(database, grammar, s, bare, root, aliases={})
        record["dependencies"] = sorted({n.label for n in proof_steps(root) if database[n.label].kind in ("$a", "$p")}) if root else []
        record["bare"] = bare
        if uncovered:
            record["omitted_dv_pairs"] = uncovered
        pieces[s.label] = piece
        translator.bare[s.label] = bare

    def ensure_variant(source, pattern):
        """Emit dependencies recursively, including cases outside the eager limit."""
        family = translator.variants[source.label]
        if pattern in family:
            return
        bare = translator.required_bare[source.label]
        variant = specialize_statement(database, source, bare, pattern)
        family[pattern] = variant
        s = variant.statement
        mapped_dv = {tuple(sorted(variant.renaming.get(v, v) for v in p)) for p in source.dv}
        record = {"kind": source.kind, "status": "candidate", "auxiliary": True,
                  "source_label": source.label, "identifications": variant.renaming,
                  "additional_dv_pairs": sorted(s.dv - mapped_dv)}
        records[s.label] = record
        records[source.label]["variants"].append(s.label)
        try:
            require(source.label not in excluded and s.label not in excluded,
                    "excluded after a failed Metacat check")
            root = specialize_proof(translator.template(source.label), variant) if source.kind == "$p" else None
            emit(s, root, variant.bare, record)
        except (TranslationError, RecursionError) as error:
            record.update(status="translation_failed", reason=str(error) or "recursion depth exceeded")

    translator.ensure_variant = ensure_variant
    for s in statements:
        if s.kind not in ("$a", "$p"):
            continue
        processed += 1
        record = {"kind": s.kind, "status": "candidate"}
        records[s.label] = record
        sources[s.label] = s
        try:
            root = translator.template(s.label) if s.kind == "$p" else None
            bare = translator.requirements(s, root)
            setvars = [h.expression[1] for h in floats(database, s) if h.expression[0] == "setvar"]
            split = any(x != y and tuple(sorted((x, y))) not in s.dv for x in bare for y in setvars)
            if split:
                translator.variants[s.label] = {}
                record.update(variants=[], enumeration_complete=False, bare=bare)
                for index, pattern in enumerate(unbundled_patterns(setvars, bare, s.dv)):
                    if max_variants and index >= max_variants:
                        break
                    ensure_variant(s, pattern)
                else:
                    record["enumeration_complete"] = True
            else:
                require(s.label not in excluded, "excluded after a failed Metacat check")
                emit(s, root, bare, record)
        except (TranslationError, RecursionError) as error:
            record.update(status="translation_failed", reason=str(error) or "recursion depth exceeded")
            if "variants" in record:
                record["enumeration_error"] = record["reason"]
        if processed % 1000 == 0:
            print(f"Translated {processed} source assertions ({len(pieces)} declarations emitted), {time.monotonic()-started:.1f}s", file=sys.stderr, flush=True)
        if s.label == through:
            break
    # On-demand generation may have completed a family after its eager limit.
    for label, family in translator.variants.items():
        record = records[label]
        if not record["enumeration_complete"] and "enumeration_error" not in record:
            source = sources[label]
            setvars = [h.expression[1] for h in floats(database, source) if h.expression[0] == "setvar"]
            record["enumeration_complete"] = all(
                pattern in family for pattern in unbundled_patterns(setvars, record["bare"], source.dv))
    summarize_bundles(records)
    return prefix, pieces, records


def write_hex(path, prefix, pieces):
    with path.open("w", encoding="utf-8", newline="\n") as output:
        output.write(prefix)
        for piece in pieces.values():
            output.write(piece + "\n")
        output.write("})\n")


def check_candidates(path, records, pieces):
    """Check actual Hex diagrams, then remove failures and all dependent proofs."""
    root = Path(__file__).resolve().parent
    command = ["cargo", "run", "--release", "--quiet", "--example", "verify_metamath", "--", str(path.resolve())]
    process = subprocess.Popen(command, cwd=root, stdout=subprocess.PIPE, text=True)
    seen = set()
    failures = {}
    try:
        for line in process.stdout:
            fields = line.rstrip("\n").split("\t", 2)
            require(len(fields) == 3 and fields[0] in ("OK", "FAIL"), f"invalid checker output: {line.rstrip()}")
            status, label, detail = fields
            require(label not in seen, f"duplicate checker result for {label}")
            seen.add(label)
            if label in ("fresh", "fresh-wff", "fresh-class"):
                require(status == "OK", f"local-variable introduction failed: {detail}")
                continue
            require(label in pieces, f"unexpected checker result for {label}")
            record = records[label]
            if status == "OK":
                record["status"] = "proved" if record["kind"] == "$p" else "axiom"
            else:
                record.update(status="check_failed", reason=detail)
                failures[label] = detail
        require(process.wait() == 0, "Metacat checker did not complete successfully")
        require(set(pieces) <= seen, "Metacat checker omitted some declarations")
    finally:
        process.stdout.close()
        if process.poll() is None:
            process.terminate()
            process.wait()

    return retain_checked(records, pieces), failures


def retain_checked(records, pieces):
    """Prune checked-looking declarations whose proof depends on a failure."""
    # On-demand variants are emitted before their callers, so insertion order
    # remains topological even when it differs from source declaration order.
    retained = {}
    for label, piece in pieces.items():
        record = records[label]
        if record["status"] not in ("proved", "axiom"):
            continue
        unavailable = [d for d in record["dependencies"] if d not in retained]
        if unavailable:
            record.update(status="dependency_failed", reason="unavailable checked dependencies: " + ", ".join(unavailable))
        else:
            retained[label] = piece
    return retained


def summarize_bundles(records):
    """Count a source assertion only when every identification case is covered."""
    for record in records.values():
        if "variants" not in record:
            continue
        variants = [records[name] for name in record["variants"]]
        failed = [name for name in record["variants"]
                  if records[name]["status"] not in ("candidate", "proved", "axiom")]
        if "enumeration_error" in record:
            record.update(status="translation_failed", reason=record["enumeration_error"])
        elif not record["enumeration_complete"]:
            record.update(status="unbundling_limited", reason="variant enumeration limit reached; family is incomplete")
        elif failed:
            record.update(status="translation_failed", reason="unavailable variants: " + ", ".join(failed))
        elif variants and all(v["status"] in ("proved", "axiom") for v in variants):
            record["status"] = "proved" if record["kind"] == "$p" else "axiom"
            record.pop("reason", None)
        else:
            record["status"] = "candidate"
            record.pop("reason", None)


def write_report(path, source_hash, records, checked):
    summarize_bundles(records)
    assertions = {k: v for k, v in records.items() if not v.get("auxiliary")}
    auxiliaries = {k: v for k, v in records.items() if v.get("auxiliary")}
    counts = defaultdict(int)
    for record in assertions.values():
        counts[record["status"]] += 1
    report = {"source_sha256": source_hash, "metacat_checked": checked, "counts": dict(counts),
              "assertions": assertions, "auxiliaries": auxiliaries}
    path.with_suffix(".report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    missing = path.with_name(path.stem + "-untranslated.tsv")
    with missing.open("w", encoding="utf-8") as output:
        output.write("theorem\tstatus\treason\tchecked_variants\n")
        for label, record in assertions.items():
            if record["kind"] == "$p" and record["status"] not in ("proved", "candidate"):
                reason = record.get("reason", "").replace("\t", " ").replace("\n", " ")
                variants = [v for v in record.get("variants", ()) if auxiliaries[v]["status"] == "proved"]
                output.write(f"{label}\t{record['status']}\t{reason}\t{','.join(variants)}\n")
    return dict(counts)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("source", type=Path)
    parser.add_argument("-o", "--output", type=Path, help="default: examples/set.hex (mini-set.hex with --mini)")
    parser.add_argument("--mini", action="store_true", help="translate the foundational/sp fragment; default: examples/mini-set.hex")
    parser.add_argument("--through", help="stop after this assertion (for incremental imports)")
    parser.add_argument("--max-steps", type=int, default=20000, help="maximum expanded proof steps; 0 is unlimited")
    parser.add_argument("--max-variants", type=int, default=128,
                        help="eager variants per assertion; more generated on demand; 0 is unlimited")
    parser.add_argument("--exclude", type=Path, help="newline-separated labels to inline rather than emit")
    parser.add_argument("--check", action="store_true", help="retain only Metacat-checked proofs with checked dependencies")
    parser.add_argument("--passes", type=int, default=2, help="checking passes; inline failed lemmas on subsequent passes")
    args = parser.parse_args(argv)
    args.output = args.output or Path("examples/mini-set.hex" if args.mini else "examples/set.hex")
    try:
        outputs = [args.output]
        if not args.mini:
            outputs.extend([args.output.with_suffix(".report.json"), args.output.with_name(args.output.stem + "-untranslated.tsv")])
        require(all(path.resolve() != args.source.resolve() for path in outputs),
                "output must not overwrite the source database")
        require(args.max_steps >= 0 and args.max_variants >= 0 and args.passes >= 1, "invalid limit or pass count")
        data = args.source.read_bytes()
        database = parse_database(data.decode("utf-8"))
        require(not args.through or args.through in database, f"unknown stopping label {args.through}")
        if args.mini:
            require(not args.check, "check the mini fragment separately with cargo run -- check")
            args.output.write_text(generate(database, hashlib.sha256(data).hexdigest()), encoding="utf-8")
            return 0
        excluded = set(args.exclude.read_text().splitlines()) if args.exclude else set()
        source_hash = hashlib.sha256(data).hexdigest()
        failed_checks = {}
        for attempt in range(args.passes if args.check else 1):
            prefix, pieces, records = generate_all(database, source_hash, excluded, args.max_steps,
                                                  args.through, args.max_variants)
            if not args.check:
                break
            print(f"Metacat checking pass {attempt + 1}", file=sys.stderr, flush=True)
            with tempfile.TemporaryDirectory(prefix="metacat-import-") as temporary:
                candidate = Path(temporary) / "candidate.hex"
                write_hex(candidate, prefix, pieces)
                retained, failures = check_candidates(candidate, records, pieces)
            pieces = retained
            failed_checks.update(failures)
            if not failures:
                break
            excluded.update(failures)
        for label, reason in failed_checks.items():
            if label in records and records[label]["status"] == "translation_failed":
                records[label].update(status="check_failed", reason=reason)
        if args.check:
            prefix = "# Every emitted proof and its dependencies passed Metacat DV checking.\n" + prefix
        write_hex(args.output, prefix, pieces)
        counts = write_report(args.output, source_hash, records, args.check)
        source_total = sum(not record.get("auxiliary") for record in records.values())
        helpers = sum(records[name].get("auxiliary", False) for name in pieces)
        print(f"Emitted {len(pieces)} declarations ({helpers} unbundled variants) for {source_total} source assertions to {args.output}: {counts}", file=sys.stderr)
        if not args.check:
            print("Metacat checking still required; rerun with --check", file=sys.stderr)
        return 0
    except (OSError, UnicodeError, TranslationError) as error:
        parser.exit(1, f"translation failed: {error}\n")


if __name__ == "__main__":
    sys.exit(main())
