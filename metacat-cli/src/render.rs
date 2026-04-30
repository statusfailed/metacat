use metacat::check::{RawTypeTerm, TypeMapComponent};
use metacat::theory::OperationKey;
use open_hypergraphs::lax::OpenHypergraph;

pub fn print_dot_hypergraph<O, A>(f: &OpenHypergraph<O, A>)
where
    O: Clone + PartialEq + std::fmt::Debug,
    A: Clone + PartialEq + std::fmt::Display + std::fmt::Debug,
{
    use graphviz_rust::printer::{DotPrinter, PrinterContext};
    use open_hypergraphs_dot::{Options, generate_dot_with};

    let mut opts = Options::default();
    opts.edge_label = Box::new(|edge| format!("{edge}"));

    let graph = generate_dot_with(f, &opts);
    let mut ctx = PrinterContext::default();
    println!("{}", graph.print(&mut ctx));
}

pub fn print_dot_raw_type_map<O>(raw: &RawTypeTerm<O>)
where
    O: Clone + PartialEq + std::fmt::Display + std::fmt::Debug,
{
    println!("digraph G {{");
    println!("  rankdir=TB");
    println!("  bgcolor=\"#4a4a4a\"");
    println!("  node[shape=record style=rounded fontcolor=\"white\" color=\"white\"]");
    println!("  edge[fontcolor=\"white\" color=\"white\" arrowhead=none]");
    println!();

    print_dot_cluster("source+", "source", &raw.graph, &raw.source);
    print_dot_cluster("proof type-map", "proof", &raw.graph, &raw.proof);
    print_dot_cluster("target-", "target", &raw.graph, &raw.target);

    println!(
        "  sources[label=\"{{ {{}} | {{ {} }} }}\" shape=record style=invisible rank=source]",
        ports("p", raw.graph.sources.len())
    );
    for (i, node) in raw.graph.sources.iter().enumerate() {
        println!("  sources:p_{i} -> n_{} [style=dashed]", node.0);
    }

    println!(
        "  targets[label=\"{{ {{ {} }} | {{}} }}\" shape=record style=invisible rank=sink]",
        ports("p", raw.graph.targets.len())
    );
    for (i, node) in raw.graph.targets.iter().enumerate() {
        println!("  n_{} -> targets:p_{i} [style=dashed]", node.0);
    }

    for i in 0..raw.graph.hypergraph.edges.len() {
        let adjacency = &raw.graph.hypergraph.adjacency[i];
        for (j, node) in adjacency.sources.iter().enumerate() {
            println!("  n_{} -> e_{i}:s_{j}", node.0);
        }
        for (j, node) in adjacency.targets.iter().enumerate() {
            println!("  e_{i}:t_{j} -> n_{}", node.0);
        }
    }

    for (source, target) in raw
        .graph
        .hypergraph
        .quotient
        .0
        .iter()
        .zip(raw.graph.hypergraph.quotient.1.iter())
    {
        println!("  n_{} -> n_{} [style=dotted dir=none]", source.0, target.0);
    }

    println!("}}");
}

fn print_dot_cluster<O>(
    label: &str,
    name: &str,
    graph: &OpenHypergraph<(), metacat::dual::Dual<O>>,
    component: &TypeMapComponent,
) where
    O: Clone + PartialEq + std::fmt::Display + std::fmt::Debug,
{
    println!("  subgraph cluster_{name} {{");
    println!("    label=\"{}\"", escape_dot_string(label));
    println!("    color=\"white\"");
    println!("    fontcolor=\"white\"");
    println!("    style=\"dashed\"");

    for i in component.node_range.clone() {
        println!("    n_{i}[shape=point xlabel=\"()\"]");
    }

    for i in component.edge_range.clone() {
        let edge = &graph.hypergraph.edges[i];
        let adjacency = &graph.hypergraph.adjacency[i];
        println!(
            "    e_{i}[label=\"{{ {{ {} }} | {} | {{ {} }} }}\" shape=record]",
            ports("s", adjacency.sources.len()),
            escape_record_text(&format!("{edge}")),
            ports("t", adjacency.targets.len())
        );
    }

    println!("  }}");
    println!();
}

fn ports(prefix: &str, count: usize) -> String {
    (0..count)
        .map(|i| format!("<{prefix}_{i}>"))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn escape_dot_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_record_text(value: &str) -> String {
    escape_dot_string(value)
        .replace('{', "\\{")
        .replace('}', "\\}")
        .replace('|', "\\|")
        .replace('<', "\\<")
        .replace('>', "\\>")
}

pub fn print_open_hypergraph<O: std::fmt::Debug, A: std::fmt::Display>(f: &OpenHypergraph<O, A>) {
    let sources: Vec<String> = f
        .sources
        .iter()
        .map(|node| format!("v{}", node.0))
        .collect();
    let targets: Vec<String> = f
        .targets
        .iter()
        .map(|node| format!("v{}", node.0))
        .collect();
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
