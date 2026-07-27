//! Make implicit Frobenius structure explicit in lax open hypergraphs.
//!
//! A lax [`OpenHypergraph`] can represent wire sharing in two ways:
//! - a single node can be used by several boundary or operation ports;
//! - the lax quotient relation can identify several nodes.
//!
//! For proof checking we need that sharing to be evaluated locally, as ordinary
//! operations. This module turns each quotient-connected component of nodes into
//! one explicit spider edge, and gives every port occurrence its own node.

use open_hypergraphs::lax::{Hyperedge, Hypergraph, NodeId, OpenHypergraph};

/// Edge labels for a hypergraph whose implicit sharing has been made explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WithSpiders<O, A> {
    /// An explicit spider for one original node-equivalence class.
    Spider(O),
    /// An original operation edge.
    Operation(A),
}

/// Error returned when an implicit spider component has inconsistent labels.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExtractSpidersError {
    #[error("Cannot extract spiders from a component containing differently labeled nodes")]
    LabelMismatch,
}

/// Result of spider extraction, including a representative for each input node.
#[derive(Clone, Debug, PartialEq)]
pub struct SpiderExtraction<O, A> {
    pub graph: OpenHypergraph<O, WithSpiders<O, A>>,
    pub node_map: Vec<Option<NodeId>>,
}

/// Replace all implicit node sharing in `f` by explicit spider edges.
///
/// The returned graph has an empty quotient relation. Each original operation
/// port and each boundary occurrence receives a fresh node. For each connected
/// component of original nodes, one spider connects:
///
/// - open sources and operation outputs as spider sources;
/// - operation inputs and open targets as spider targets.
pub fn extract_spiders<O, A>(
    f: &OpenHypergraph<O, A>,
) -> Result<OpenHypergraph<O, WithSpiders<O, A>>, ExtractSpidersError>
where
    O: Clone + PartialEq,
    A: Clone,
{
    extract_spiders_with_node_map(f).map(|extraction| extraction.graph)
}

/// Like [`extract_spiders`], but also returns one representative output node
/// for each input node.
pub fn extract_spiders_with_node_map<O, A>(
    f: &OpenHypergraph<O, A>,
) -> Result<SpiderExtraction<O, A>, ExtractSpidersError>
where
    O: Clone + PartialEq,
    A: Clone,
{
    let original_node_count = f.hypergraph.nodes.len();
    let mut f = f.clone();
    let q = f
        .quotient()
        .map_err(|_| ExtractSpidersError::LabelMismatch)?;

    let mut spiders: Vec<Component<O>> = f
        .hypergraph
        .nodes
        .iter()
        .cloned()
        .map(|label| Component {
            label,
            sources: Vec::new(),
            targets: Vec::new(),
        })
        .collect();

    let mut out = OpenHypergraph {
        sources: Vec::new(),
        targets: Vec::new(),
        hypergraph: Hypergraph::empty(),
    };

    for source in &f.sources {
        let node = out.new_node(f.hypergraph.nodes[source.0].clone());
        out.sources.push(node);
        spiders[source.0].sources.push(node);
    }

    for (edge, adjacency) in f.hypergraph.edges.iter().zip(&f.hypergraph.adjacency) {
        let mut sources = Vec::with_capacity(adjacency.sources.len());
        for source in &adjacency.sources {
            let node = out.new_node(f.hypergraph.nodes[source.0].clone());
            sources.push(node);
            spiders[source.0].targets.push(node);
        }

        let mut targets = Vec::with_capacity(adjacency.targets.len());
        for target in &adjacency.targets {
            let node = out.new_node(f.hypergraph.nodes[target.0].clone());
            targets.push(node);
            spiders[target.0].sources.push(node);
        }

        out.new_edge(WithSpiders::Operation(edge.clone()), (sources, targets));
    }

    for target in &f.targets {
        let node = out.new_node(f.hypergraph.nodes[target.0].clone());
        out.targets.push(node);
        spiders[target.0].targets.push(node);
    }

    let representatives: Vec<Option<NodeId>> = spiders
        .iter()
        .map(|component| {
            component
                .targets
                .first()
                .or_else(|| component.sources.first())
                .copied()
        })
        .collect();
    let node_map = (0..original_node_count)
        .map(|node| representatives[q.table[node]])
        .collect();

    for component in spiders {
        if component.sources.is_empty() && component.targets.is_empty() {
            continue;
        }
        out.new_edge(
            WithSpiders::Spider(component.label),
            Hyperedge {
                sources: component.sources,
                targets: component.targets,
            },
        );
    }

    Ok(SpiderExtraction {
        graph: out,
        node_map,
    })
}

// After quotienting, each remaining node represents one implicit spider.
// This helper accumulates the fresh port-occurrence nodes that will become the
// source and target ports of that explicit spider edge.
#[derive(Clone, Debug)]
struct Component<O> {
    label: O,
    sources: Vec<NodeId>,
    targets: Vec<NodeId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::nat::{NatKey, NatObj};
    use crate::theory::{Theory, TheoryId, TheorySet};
    use hexpr::{parse_hexprs, try_interpret};

    fn nat_hexpr(text: &str) -> Result<OpenHypergraph<(), NatKey>, Box<dyn std::error::Error>> {
        let hexprs = parse_hexprs(text)?;
        let [hexpr] = hexprs.as_slice() else {
            panic!("expected one hexpr");
        };
        Ok(try_interpret(&NatObj, hexpr)?.map_nodes(|_| ()))
    }

    fn definition_body(
        body: &str,
    ) -> Result<OpenHypergraph<(), hexpr::Operation>, Box<dyn std::error::Error>> {
        let text = format!(
            r#"
            (theory test.syntax nat {{
              (arr f : 1 -> 1)
              (arr g : 1 -> 1)
            }})

            (theory test.proof test.syntax {{
              (arr f : f -> f)
              (arr g : g -> g)
              (def sample : {{f f}} -> {{g g}} = {body})
            }})
            "#
        );
        let theories = TheorySet::from_text(&text)?;
        let theory_id = TheoryId("test.proof".parse()?);
        let Theory::Theory { arrows, .. } = theories.theories.get(&theory_id).unwrap() else {
            panic!("expected user theory");
        };
        Ok(arrows
            .get(&"sample".parse()?)
            .unwrap()
            .definition
            .clone()
            .unwrap())
    }

    fn spider_profiles<O: Clone, A>(
        f: &OpenHypergraph<O, WithSpiders<O, A>>,
    ) -> Vec<(usize, usize)> {
        f.hypergraph
            .edges
            .iter()
            .zip(&f.hypergraph.adjacency)
            .filter_map(|(edge, adjacency)| match edge {
                WithSpiders::Spider(_) => Some((adjacency.sources.len(), adjacency.targets.len())),
                WithSpiders::Operation(_) => None,
            })
            .collect()
    }

    #[test]
    fn extracts_copy_spider_from_frobenius_hexpr() -> Result<(), Box<dyn std::error::Error>> {
        let map = nat_hexpr("[x . x x]")?;
        let extracted = extract_spiders(&map)?;

        assert_eq!(spider_profiles(&extracted), vec![(1, 2)]);
        assert!(extracted.hypergraph.quotient.0.is_empty());
        assert!(extracted.hypergraph.quotient.1.is_empty());
        Ok(())
    }

    #[test]
    fn extracts_merge_spider_from_frobenius_hexpr() -> Result<(), Box<dyn std::error::Error>> {
        let map = nat_hexpr("[a a . a]")?;
        let extracted = extract_spiders(&map)?;

        assert_eq!(spider_profiles(&extracted), vec![(2, 1)]);
        Ok(())
    }

    #[test]
    fn extracts_spiders_from_composition_quotient() -> Result<(), Box<dyn std::error::Error>> {
        let body = definition_body("({f f} {g g})")?;
        assert!(!body.hypergraph.quotient.0.is_empty());

        let extracted = extract_spiders(&body)?;

        assert_eq!(extracted.sources.len(), 2);
        assert_eq!(extracted.targets.len(), 2);
        assert_eq!(
            extracted
                .hypergraph
                .edges
                .iter()
                .filter(|edge| matches!(edge, WithSpiders::Operation(_)))
                .count(),
            4
        );
        assert_eq!(
            spider_profiles(&extracted),
            vec![(1, 1), (1, 1), (1, 1), (1, 1), (1, 1), (1, 1)]
        );
        assert!(extracted.hypergraph.quotient.0.is_empty());
        assert!(extracted.hypergraph.quotient.1.is_empty());
        Ok(())
    }

    #[test]
    fn extracts_two_by_two_spider_between_tensor_layers() -> Result<(), Box<dyn std::error::Error>>
    {
        let body = definition_body("({f f} [x x . x x] {g g})")?;
        let extracted = extract_spiders(&body)?;

        assert_eq!(extracted.sources.len(), 2);
        assert_eq!(extracted.targets.len(), 2);
        assert_eq!(
            extracted
                .hypergraph
                .edges
                .iter()
                .filter(|edge| matches!(edge, WithSpiders::Operation(_)))
                .count(),
            4
        );
        assert!(spider_profiles(&extracted).contains(&(2, 2)));
        assert!(extracted.hypergraph.quotient.0.is_empty());
        assert!(extracted.hypergraph.quotient.1.is_empty());
        Ok(())
    }
}
