use metacat::theory::OperationKey;
use open_hypergraphs::lax::OpenHypergraph;

pub fn print_open_hypergraph<O: std::fmt::Debug, A: std::fmt::Display>(
    f: &OpenHypergraph<O, A>,
) {
    let sources: Vec<String> = f.sources.iter().map(|node| format!("v{}", node.0)).collect();
    let targets: Vec<String> = f.targets.iter().map(|node| format!("v{}", node.0)).collect();
    println!("  sources: [{}]", sources.join(", "));
    println!("  targets: [{}]", targets.join(", "));

    println!("  nodes:");
    for (i, node) in f.hypergraph.nodes.iter().enumerate() {
        println!("    v{i}: {node:?}");
    }

    println!("  edges:");
    if f.hypergraph.edges.is_empty() {
        println!("    <none>");
    }
    for (i, edge) in f.hypergraph.edges.iter().enumerate() {
        let adjacency = &f.hypergraph.adjacency[i];
        let edge_sources: Vec<String> = adjacency
            .sources
            .iter()
            .map(|node| format!("v{}", node.0))
            .collect();
        let edge_targets: Vec<String> = adjacency
            .targets
            .iter()
            .map(|node| format!("v{}", node.0))
            .collect();
        println!(
            "    e{i}: [{}] --{}--> [{}]",
            edge_sources.join(", "),
            edge,
            edge_targets.join(", ")
        );
    }

    if !f.hypergraph.quotient.0.is_empty() {
        let pairs: Vec<String> = f
            .hypergraph
            .quotient
            .0
            .iter()
            .zip(f.hypergraph.quotient.1.iter())
            .map(|(a, b)| format!("v{}=v{}", a.0, b.0))
            .collect();
        println!("  pending quotient: {}", pairs.join(", "));
    }
}

pub fn print_state(
    label: &str,
    state: &[Option<metacat::tree::Tree<(), OperationKey>>],
    coarity: &dyn Fn(&OperationKey) -> usize,
) {
    let values: Vec<String> = state
        .iter()
        .enumerate()
        .filter_map(|(i, value)| {
            value
                .as_ref()
                .map(|tree| format!("v{i}={}", tree.pretty(Some(coarity))))
        })
        .collect();
    if values.is_empty() {
        println!("{label}: <empty>");
    } else {
        println!("{label}: {}", values.join(", "));
    }
}
