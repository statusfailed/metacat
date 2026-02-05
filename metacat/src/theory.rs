use hexpr::*;
use open_hypergraphs::category::Arrow;
use open_hypergraphs::lax::OpenHypergraph;
use std::collections::HashMap;

pub type ObjTerm<A> = OpenHypergraph<(), A>;

// NOTE: Operation must not be made public; user cannot construct this!
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationKey(Operation);

impl std::fmt::Display for OperationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}

#[derive(Clone, Default)]
pub struct Theory<A> {
    operations: HashMap<OperationKey, (ObjTerm<A>, ObjTerm<A>)>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No such operation {0}")]
    NoSuchOperation(OperationKey),
    #[error("Operation {0}: source and target maps must have same domain")]
    InvalidType(Operation),
}

impl<A> Theory<A> {
    pub fn new() -> Theory<A> {
        Self {
            operations: HashMap::new(),
        }
    }

    pub fn operations(&self) -> impl Iterator<Item = &OperationKey> {
        self.operations.keys()
    }

    pub fn type_maps(&self, op: &OperationKey) -> &(OpenHypergraph<(), A>, OpenHypergraph<(), A>) {
        &self.operations[op]
    }
}

impl<A: Clone> Theory<A> {
    pub fn add_operation(
        &mut self,
        name: Operation,
        source: OpenHypergraph<(), A>,
        target: OpenHypergraph<(), A>,
    ) -> Result<(), Error> {
        if source.source() != target.source() {
            return Err(Error::InvalidType(name));
        }
        assert_eq!(source.source(), target.source());
        self.operations.insert(OperationKey(name), (source, target));
        Ok(())
    }
}

impl<A: Clone> Signature for Theory<A> {
    type Arr = OperationKey;
    type Obj = ();
    type Error = Error;

    fn try_parse_op(&self, op: &Operation) -> Result<Self::Arr, Self::Error> {
        let k = OperationKey(op.clone());
        match self.operations.get(&k) {
            Some(_) => Ok(k),
            None => Err(Error::NoSuchOperation(k)),
        }
    }

    fn profile(&self, op: &Self::Arr) -> (Vec<Option<Self::Obj>>, Vec<Option<Self::Obj>>) {
        let (source_map, target_map) = &self.operations[op];
        // NOTE: the interface is the *target* of the source and target maps!
        (
            source_map.target().iter().cloned().map(Some).collect(),
            target_map.target().iter().cloned().map(Some).collect(),
        )
    }
}
