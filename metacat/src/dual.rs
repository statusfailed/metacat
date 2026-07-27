use hexpr::{Hexpr, Operation, Signature};
use open_hypergraphs::lax::{Hyperedge, Hypergraph, OpenHypergraph};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Dual<A> {
    Fwd(A),
    Rev(A),
}

impl<A> Dual<A> {
    fn dual(self) -> Self {
        match self {
            Self::Fwd(a) => Self::Rev(a),
            Self::Rev(a) => Self::Fwd(a),
        }
    }
}

pub fn into_fwd<O, A>(f: OpenHypergraph<O, A>) -> OpenHypergraph<O, Dual<A>> {
    f.map_edges(Dual::Fwd)
}

pub fn into_rev<O, A: Clone>(f: OpenHypergraph<O, A>) -> OpenHypergraph<O, Dual<A>> {
    dual(f.map_edges(Dual::Fwd))
}

pub fn dual<O, A: Clone>(f: OpenHypergraph<O, Dual<A>>) -> OpenHypergraph<O, Dual<A>> {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DualSig<S>(S);

impl<S: Signature> Signature for DualSig<S> {
    type Arr = Dual<S::Arr>;
    type Obj = S::Obj;
    type Error = S::Error;

    fn try_parse_op(&self, op: &Operation) -> Result<Self::Arr, Self::Error> {
        Ok(Dual::Fwd(self.0.try_parse_op(op)?))
    }

    fn try_parse_object(&self, object: &Hexpr) -> Result<Self::Obj, Self::Error> {
        self.0.try_parse_object(object)
    }

    fn profile(&self, op: &Self::Arr) -> (Vec<Option<Self::Obj>>, Vec<Option<Self::Obj>>) {
        match op {
            Dual::Fwd(op) => self.0.profile(op),
            Dual::Rev(op) => {
                let (s, t) = self.0.profile(op);
                (t, s)
            }
        }
    }
}

impl<T: std::fmt::Display> std::fmt::Display for Dual<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Dual::Fwd(x) => write!(f, "fwd({x})"),
            Dual::Rev(x) => write!(f, "rev({x})"),
        }
    }
}
