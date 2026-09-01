use crate::dual::{self, Dual};
use crate::theory::{Term, Theory};
use hexpr::Operation;
use open_hypergraphs::category::Arrow;
use open_hypergraphs::lax::OpenHypergraph;
use open_hypergraphs::lax::functor::{self, Functor};
use open_hypergraphs::strict::vec::FiniteFunction;
use thiserror::Error;

pub type CheckGraph = OpenHypergraph<(), Dual<Operation>>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Type maps had invalid arity/coarity")]
    InvalidTypeMaps,
    #[error("Unable to quotient graph {0:?}")]
    InvalidQuotient(FiniteFunction),
}

/// Map a derivation body `f` to `Syntax(f)` in the polarized syntax category.
pub fn syntax(theory: &Theory, mut arrow: Term) -> Result<CheckGraph, Error> {
    arrow.quotient().map_err(Error::InvalidQuotient)?;
    functor::try_define_map_arrow(&AsType(theory), &arrow).ok_or(Error::InvalidTypeMaps)
}

/// Construct `Check(d) = s+ ; Syntax(f) ; t-` for `d = (f, s, t)`.
pub fn check_graph(
    theory: &Theory,
    mut source: Term,
    mut target: Term,
    arrow: Term,
) -> Result<CheckGraph, Error> {
    source.quotient().map_err(Error::InvalidQuotient)?;
    target.quotient().map_err(Error::InvalidQuotient)?;

    let syntax = syntax(theory, arrow)?;
    let mut graph = dual::into_fwd(source)
        .lax_compose(&syntax)
        .and_then(|graph| graph.lax_compose(&dual::into_rev(target)))
        .ok_or(Error::InvalidTypeMaps)?;
    graph.quotient().map_err(Error::InvalidQuotient)?;
    Ok(graph)
}

/// Map generating arrows of a theory to their polarized type maps `s- ; t+`.
#[derive(Clone)]
struct AsType<'a>(&'a Theory);

impl Functor<(), Operation, (), Dual<Operation>> for AsType<'_> {
    fn map_object(&self, _: &()) -> impl ExactSizeIterator<Item = ()> {
        std::iter::once(())
    }

    fn map_operation(&self, operation: &Operation, source: &[()], target: &[()]) -> CheckGraph {
        let arrow = self
            .0
            .get_arrow(operation)
            .expect("missing arrow in theory");
        let (s, t) = &arrow.type_maps;

        assert_eq!(source.len(), s.targets.len());
        assert_eq!(target.len(), t.targets.len());

        dual::into_rev(s.clone())
            .compose(&dual::into_fwd(t.clone()))
            .expect("type-map boundaries should compose")
    }

    fn map_arrow(&self, arrow: &Term) -> CheckGraph {
        functor::try_define_map_arrow(self, arrow).expect("arrow should be quotiented")
    }
}
