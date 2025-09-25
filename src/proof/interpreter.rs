//! Simple interpreter for Bayesian Boolean Circuits
use super::types::*;
use crate::core;
use crate::definition::Def;
use crate::ssa::{SSA, parallel_ssa};

use open_hypergraphs::lax::NodeId;

pub type Value<T> = core::interpreter::Value<T>;

#[derive(Debug, Clone, PartialEq)]
pub struct Interpreter<T> {
    env: Env<T>,
}

impl<T> Interpreter<T> {
    pub fn new(env: Env<T>) -> Self {
        Interpreter { env }
    }
}

impl<T: Clone + PartialEq> Interpreter<T> {
    /// Run the interpreter with specified input values
    pub fn run(
        &mut self,
        mut term: Proof,
        values: Vec<Value<T>>,
    ) -> Result<Vec<Value<T>>, InterpreterError> {
        assert_eq!(values.len(), term.sources.len());

        term.quotient();

        // create initial state by moving argument values into state
        //let mut state = HashMap::<NodeId, Value>::new();
        let mut state: Vec<Option<Value<T>>> = vec![None; term.hypergraph.nodes.len()];

        for (node_id, value) in term.sources.iter().zip(values) {
            state[node_id.0] = Some(value);
        }

        // Save target nodes before moving term
        let target_nodes = term.targets.clone();

        // Iterate through partially-ordered SSA ops
        for par in parallel_ssa(term.to_strict())? {
            for op in par {
                // get args: Vec<Value> by popping each id in op.sources from state
                let mut args = Vec::new();
                for (node_id, _) in &op.sources {
                    match &state[node_id.0] {
                        Some(value) => args.push(value.clone()),
                        None => return Err(InterpreterError::NonMonogamousRead(*node_id)),
                    }
                }

                let results = self.apply(&op, args)?;

                // write each result into state at op.targets ids
                for ((node_id, _), result) in op.targets.iter().zip(results) {
                    if state[node_id.0].is_some() {
                        return Err(InterpreterError::NonMonogamousWrite(*node_id));
                    } else {
                        state[node_id.0] = Some(result);
                    }
                }
            }
        }

        // Extract target values and return them
        let mut target_values = Vec::new();
        for target_node in &target_nodes {
            match &state[target_node.0] {
                Some(value) => target_values.push(value.clone()),
                None => return Err(InterpreterError::NonMonogamousRead(*target_node)),
            }
        }

        state
            .into_iter()
            .enumerate()
            .map(|(i, v)| v.ok_or(InterpreterError::MissingValue(NodeId(i))))
            .collect()
    }

    pub fn apply(
        &mut self,
        ssa: &SSA<Obj, Def<Path, Prim>>,
        args: Vec<Value<T>>,
    ) -> Result<Vec<Value<T>>, InterpreterError> {
        match &ssa.op {
            // Run interpreter on this definition's 'core' value (both axioms and proofs have these!)
            Def::Def(path) => {
                let mut interpreter = core::Interpreter::new();
                interpreter
                    .run(self.env[path].clone(), args)
                    .map_err(|_| InterpreterError::CoreError)
            }
            Def::Arr(Prim::Copy) => todo!(),
        }
    }
}

/// TODO: split to ApplyError/InterpreterError. Latter yields SSA location information.
#[derive(Debug, Clone)]
pub enum InterpreterError {
    /// A value (identified by a node id) was written to multiple times
    NonMonogamousWrite(NodeId),

    /// a value either didn't exist or was already used
    NonMonogamousRead(NodeId),

    /// Wrong number of arguments
    ArityError {
        expected: usize,
        got: usize,
    },

    /// Type error
    TypeError,

    MissingValue(NodeId),

    /// SSA Conversion error
    SSAError(crate::ssa::SSAError),

    CoreError,
}

impl std::fmt::Display for InterpreterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterpreterError::NonMonogamousWrite(id) => {
                write!(f, "Value written multiple times: {:?}", id)
            }
            InterpreterError::NonMonogamousRead(id) => {
                write!(f, "Value read multiple times or doesn't exist: {:?}", id)
            }
            InterpreterError::ArityError { expected, got } => {
                write!(f, "Expected {} arguments, got {}", expected, got)
            }
            InterpreterError::TypeError => {
                write!(f, "Type error")
            }
            InterpreterError::SSAError(e) => write!(f, "SSA error: {:?}", e),
            InterpreterError::MissingValue(i) => write!(f, "Missing node value {}", i.0),
            InterpreterError::CoreError => write!(f, "Core Error"),
        }
    }
}

impl std::error::Error for InterpreterError {}

impl From<crate::ssa::SSAError> for InterpreterError {
    fn from(err: crate::ssa::SSAError) -> Self {
        InterpreterError::SSAError(err)
    }
}
