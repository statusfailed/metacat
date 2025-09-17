use open_hypergraphs::lax::{Hyperedge, Hypergraph, OpenHypergraph};

use crate::lang::{Arr, Obj};

pub fn dual<T: Clone>(f: OpenHypergraph<Obj, Arr<T>>) -> OpenHypergraph<Obj, Arr<T>> {
    let OpenHypergraph {
        sources,
        targets,
        hypergraph:
            Hypergraph {
                nodes,
                edges,
                adjacency,
                quotient,
            },
    } = f;

    let adj = adjacency
        .into_iter()
        .map(|Hyperedge { sources, targets }| Hyperedge {
            sources: targets,
            targets: sources,
        })
        .collect();

    OpenHypergraph {
        sources: targets,
        targets: sources,
        hypergraph: Hypergraph {
            nodes,
            edges: edges.into_iter().map(|e| e.dual()).collect(),
            adjacency: adj,
            quotient,
        },
    }
}
