//! Simple interpreter for Bayesian Boolean Circuits
use crate::fol::FOL;
use crate::lang::{Arr, Obj, Term};
use crate::ssa::{SSA, parallel_ssa};
use crate::tree::Tree;

use open_hypergraphs::lax::NodeId;
use std::collections::HashMap;

pub type Value = Tree<Obj, FOL>;

pub struct Interpreter;

impl Interpreter {
    pub fn run(
        &mut self,
        term: Term<FOL>,
        values: Vec<Value>,
    ) -> Result<Vec<Value>, InterpreterError> {
        Ok(self.run_state(term, values)?.1)
    }

    /// Run the interpreter with specified input values
    pub fn run_state(
        &mut self,
        mut term: Term<FOL>,
        values: Vec<Value>,
    ) -> Result<(Vec<Value>, Vec<Value>), InterpreterError> {
        assert_eq!(values.len(), term.sources.len());

        term.quotient();

        // create initial state by moving argument values into state
        //let mut state = HashMap::<NodeId, Value>::new();
        let mut state: Vec<Option<Value>> = vec![None; term.hypergraph.nodes.len()];

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

        match state.into_iter().collect() {
            Some(state) => Ok((state, target_values)),
            None => Err(InterpreterError::NotAllValuesComputed),
        }
    }

    pub fn apply(
        &mut self,
        ssa: &SSA<Obj, Arr<FOL>>,
        args: Vec<Value>,
    ) -> Result<Vec<Value>, InterpreterError> {
        match &ssa.op {
            Arr::Copy => self.apply_copy(ssa, args),
            Arr::Equal => self.apply_equal(ssa, args),
            Arr::Fwd(label) => self.apply_fwd(ssa, label.clone(), args),
            Arr::Rev(label) => self.apply_rev(ssa, label.clone(), args),
        }
    }

    fn apply_copy(
        &mut self,
        ssa: &SSA<Obj, Arr<FOL>>,
        mut args: Vec<Value>,
    ) -> Result<Vec<Value>, InterpreterError> {
        if args.len() != 1 {
            return Err(InterpreterError::ArityError {
                expected: 1,
                got: args.len(),
            });
        }

        // NOTE: this works when no outputs- empty vec (discards!)
        let value = args.pop().unwrap();
        Ok(vec![value; ssa.targets.len()])
    }

    fn apply_equal(
        &mut self,
        ssa: &SSA<Obj, Arr<FOL>>,
        mut args: Vec<Value>,
    ) -> Result<Vec<Value>, InterpreterError> {
        // Equal with no inputs should return a Leaf for each output
        if args.is_empty() {
            let mut results = Vec::new();
            for (node_id, obj) in &ssa.targets {
                // Create a leaf with id from targets and label from ssa
                let leaf = Tree::Leaf(node_id.0, obj.clone());
                results.push(leaf);
            }
            return Ok(results);
        }

        let first = &args[0];
        for (i, arg) in args[1..].iter().enumerate() {
            if arg != first {
                return Err(InterpreterError::UnifyError(args.swap_remove(i + 1)));
            }
        }

        let value = args.into_iter().next().unwrap();
        Ok(vec![value; ssa.targets.len()])
    }

    /// Wrap all the args under the provided label
    fn apply_fwd(
        &mut self,
        _ssa: &SSA<Obj, Arr<FOL>>,
        label: FOL,
        args: Vec<Value>,
    ) -> Result<Vec<Value>, InterpreterError> {
        let tree = Tree::Node(label, args);
        Ok(vec![tree])
    }

    /// *Unpack* all the children of a tree, provided the label matches expected
    fn apply_rev(
        &mut self,
        _ssa: &SSA<Obj, Arr<FOL>>,
        label: FOL,
        args: Vec<Value>,
    ) -> Result<Vec<Value>, InterpreterError> {
        if args.len() != 1 {
            return Err(InterpreterError::ArityError {
                expected: 1,
                got: args.len(),
            });
        }

        let mut args = args;
        match args.pop().unwrap() {
            Tree::Node(actual, children) if actual == label => Ok(children),
            Tree::Node(actual, _children) => Err(InterpreterError::TypeError {
                expected: label.clone(),
                got: Some(actual),
            }),
            Tree::Leaf(_, _) => Err(InterpreterError::TypeError {
                expected: label.clone(),
                got: None,
            }),
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
    TypeError {
        expected: FOL,
        got: Option<FOL>,
    },

    /// Unification error
    UnifyError(Value),

    NotAllValuesComputed,

    /// SSA Conversion error
    SSAError(crate::ssa::SSAError),
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
            InterpreterError::TypeError { expected, got } => {
                write!(f, "Type error: expected {}, got {:?}", expected, got)
            }
            InterpreterError::UnifyError(value) => {
                write!(f, "Unification error: {}", value)
            }
            InterpreterError::SSAError(e) => write!(f, "SSA error: {:?}", e),
            InterpreterError::NotAllValuesComputed => write!(f, "Not all values computed"),
        }
    }
}

impl std::error::Error for InterpreterError {}

impl From<crate::ssa::SSAError> for InterpreterError {
    fn from(err: crate::ssa::SSAError) -> Self {
        InterpreterError::SSAError(err)
    }
}
