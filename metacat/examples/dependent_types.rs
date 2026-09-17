use hexpr::{Hexpr, Operation, Signature, try_interpret_with_names};
use metacat::check::check;
use metacat::theory::{Theory, TheoryId, TheorySet};
use open_hypergraphs::lax::NodeId;
use open_hypergraphs::strict::vec::FiniteFunction;
use std::collections::BTreeMap;

const SOURCE: &str = r#"
(theory expr nat {
  (arr u32 : 0 -> 1)
  (arr 0 : 0 -> 1)
  (arr > : 2 -> 1)
  (arr : : 2 -> 1)
})

(theory program expr {
  (arr zero : [x.] -> ({[x] u32} :))
  (arr assert-nz : ({[x] u32} :) -> ({[x] 0} >))
  (arr pred : ([x z.x x] {({_ u32} :) ({_ 0} >)}) -> ({[x z.z] u32} :))
})

(def program fail-pred : ({[y z.y] u32} :) -> ({[y z.z] u32} :) = {[y.]
    (zero [x.])
    (zero [w.])
    ([.w] assert-nz [p.])
    ([.y p] pred [z.])
[.z]})

(def program pass-pred : ({[y z.y] u32} :) -> ({[y z.z] u32} :) = {[y.]
    ([.y] assert-nz [p.])
    ([.y p] pred [z.])
[.z]})
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NodeKind {
    Value,
    Proof,
}

struct ProgramSignature;

impl Signature for ProgramSignature {
    type Arr = Operation;
    type Obj = NodeKind;
    type Error = String;

    fn try_parse_op(&self, operation: &Operation) -> Result<Self::Arr, Self::Error> {
        match operation.as_str() {
            "zero" | "assert-nz" | "pred" => Ok(operation.clone()),
            _ => Err(format!("unknown program operation '{operation}'")),
        }
    }

    fn try_parse_object(&self, object: &Hexpr) -> Result<Self::Obj, Self::Error> {
        Err(format!("unexpected explicit node label '{object}'"))
    }

    fn profile(&self, operation: &Self::Arr) -> (Vec<Option<Self::Obj>>, Vec<Option<Self::Obj>>) {
        use NodeKind::{Proof, Value};

        match operation.as_str() {
            "zero" => (vec![], vec![Some(Value)]),
            "assert-nz" => (vec![Some(Value)], vec![Some(Proof)]),
            "pred" => (vec![Some(Value), Some(Proof)], vec![Some(Value)]),
            _ => unreachable!("operations are checked by try_parse_op"),
        }
    }
}

fn value_collisions(mapping: &FiniteFunction, value_nodes: &[usize]) -> Vec<Vec<usize>> {
    let mut classes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &node in value_nodes {
        classes.entry(mapping.table.0[node]).or_default().push(node);
    }
    classes
        .into_values()
        .filter(|class| class.len() > 1)
        .collect()
}

fn check_program(
    theory: &Theory,
    name: &str,
) -> Result<Vec<Vec<String>>, Box<dyn std::error::Error>> {
    let Theory::Theory { arrows, .. } = theory else {
        unreachable!("program is a user theory")
    };
    let operation: Operation = name.parse()?;
    let declaration = arrows
        .get(&operation)
        .ok_or_else(|| format!("missing definition '{name}'"))?;
    let definition = declaration
        .raw
        .definition
        .as_ref()
        .ok_or_else(|| format!("'{name}' is not a definition"))?;

    let labeled = try_interpret_with_names(&ProgramSignature, definition)?.unify()?;
    let value_nodes: Vec<usize> = labeled
        .graph
        .hypergraph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(node, kind)| (*kind == NodeKind::Value).then_some(node))
        .collect();
    let names = labeled.names;
    let mut arrow = labeled.graph.map_nodes(|_| ());
    let (source, target) = declaration.type_maps.clone();
    let mapping = check(theory, source, target, &mut arrow)?;

    Ok(value_collisions(&mapping, &value_nodes)
        .into_iter()
        .map(|class| {
            class
                .into_iter()
                .flat_map(|node| names.get(&NodeId(node)).into_iter().flatten())
                .map(ToString::to_string)
                .collect()
        })
        .collect())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let theories = TheorySet::from_text(SOURCE)?;
    let program_id = TheoryId("program".parse()?);
    let program = theories
        .theories
        .get(&program_id)
        .ok_or("missing program theory")?;

    let pass_collisions = check_program(program, "pass-pred")?;
    assert!(pass_collisions.is_empty());
    println!("pass-pred: accepted");

    let fail_collisions = check_program(program, "fail-pred")?;
    assert!(!fail_collisions.is_empty());
    println!(
        "fail-pred: rejected (identified value nodes: {})",
        fail_collisions
            .into_iter()
            .map(|names| names.join(" = "))
            .collect::<Vec<_>>()
            .join(", ")
    );

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run()
}

#[cfg(test)]
mod tests {
    #[test]
    fn distinguishes_safe_and_unsafe_predecessor_programs() {
        super::run().expect("dependent-types example should run");
    }
}
