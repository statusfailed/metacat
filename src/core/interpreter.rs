//! Simple interpreter for Bayesian Boolean Circuits
use crate::core::{Arr, Obj, Term};
use crate::ssa::{SSA, parallel_ssa};
use crate::tree::Tree;

use open_hypergraphs::lax::NodeId;
use std::collections::HashMap;
use std::fmt::{Debug, Display};

pub type Value<T> = Tree<Obj, T>;

pub struct Interpreter<T> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Interpreter<T> {
    pub fn new() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<T: Clone + PartialEq> Interpreter<T> {
    /// Run the interpreter with specified input values
    pub fn run(
        &mut self,
        mut term: Term<T>,
        values: Vec<Value<T>>,
    ) -> Result<Vec<Value<T>>, InterpreterError<T>> {
        assert_eq!(values.len(), term.sources.len());

        term.quotient();

        // create initial state by moving argument values into state
        let mut state = HashMap::<NodeId, Value<T>>::new();
        for (node_id, value) in term.sources.iter().zip(values) {
            state.insert(*node_id, value);
        }

        // Save target nodes before moving term
        let target_nodes = term.targets.clone();

        // Iterate through partially-ordered SSA ops
        for par in parallel_ssa(term.to_strict())? {
            for op in par {
                // get args: Vec<Value> by popping each id in op.sources from state
                let mut args = Vec::new();
                for (node_id, _) in &op.sources {
                    match state.remove(node_id) {
                        Some(value) => args.push(value),
                        None => return Err(InterpreterError::NonMonogamousRead(*node_id)),
                    }
                }

                let results = self.apply(&op, args)?;

                // write each result into state at op.targets ids
                for ((node_id, _), result) in op.targets.iter().zip(results) {
                    if state.insert(*node_id, result).is_some() {
                        return Err(InterpreterError::NonMonogamousWrite(*node_id));
                    }
                }
            }
        }

        // Extract target values and return them
        let mut target_values = Vec::new();
        for target_node in &target_nodes {
            match state.remove(target_node) {
                Some(value) => target_values.push(value),
                None => return Err(InterpreterError::NonMonogamousRead(*target_node)),
            }
        }

        Ok(target_values)
    }

    pub fn apply(
        &mut self,
        ssa: &SSA<Obj, Arr<T>>,
        args: Vec<Value<T>>,
    ) -> Result<Vec<Value<T>>, InterpreterError<T>> {
        match &ssa.op {
            Arr::Copy => self.apply_copy(ssa, args),
            Arr::Equal => self.apply_equal(ssa, args),
            Arr::Fwd(label) => self.apply_fwd(ssa, label.clone(), args),
            Arr::Rev(label) => self.apply_rev(ssa, label.clone(), args),
        }
    }

    fn apply_copy(
        &mut self,
        ssa: &SSA<Obj, Arr<T>>,
        mut args: Vec<Value<T>>,
    ) -> Result<Vec<Value<T>>, InterpreterError<T>> {
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
        ssa: &SSA<Obj, Arr<T>>,
        mut args: Vec<Value<T>>,
    ) -> Result<Vec<Value<T>>, InterpreterError<T>> {
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
        _ssa: &SSA<Obj, Arr<T>>,
        label: T,
        args: Vec<Value<T>>,
    ) -> Result<Vec<Value<T>>, InterpreterError<T>> {
        let tree = Tree::Node(label, args);
        Ok(vec![tree])
    }

    /// *Unpack* all the children of a tree, provided the label matches expected
    fn apply_rev(
        &mut self,
        _ssa: &SSA<Obj, Arr<T>>,
        label: T,
        args: Vec<Value<T>>,
    ) -> Result<Vec<Value<T>>, InterpreterError<T>> {
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
pub enum InterpreterError<T> {
    /// A value (identified by a node id) was written to multiple times
    NonMonogamousWrite(NodeId),

    /// a value either didn't exist or was already used
    NonMonogamousRead(NodeId),

    /// Wrong number of arguments
    ArityError { expected: usize, got: usize },

    /// Type error
    TypeError { expected: T, got: Option<T> },

    /// Unification error
    UnifyError(Value<T>),

    /// SSA Conversion error
    SSAError(crate::ssa::SSAError),
}

impl<T: PartialEq + Clone + Display + Debug> Display for InterpreterError<T> {
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
        }
    }
}

impl<T: Display + Debug + Clone + PartialEq> std::error::Error for InterpreterError<T> {}

impl<T> From<crate::ssa::SSAError> for InterpreterError<T> {
    fn from(err: crate::ssa::SSAError) -> Self {
        InterpreterError::SSAError(err)
    }
}
