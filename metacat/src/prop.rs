//! # The theory of PROPs
//!
//! Objects are natural numbers `n` modeled as as constant maps `n : 0 → ()^n`
//! where `()` is a single generating object, and `()^n` is its n-fold tensor.
use hexpr::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nat(usize);

// The theory of *objects* of a PROP plus metavariables
pub struct PropObj;

impl Signature for PropObj {
    type Arr = Nat;
    type Obj = ();
    type Error = std::num::ParseIntError;

    fn try_parse_op(&self, op: &Operation) -> Result<Self::Arr, Self::Error> {
        Ok(Nat(op.as_str().parse()?))
    }

    fn profile(&self, op: &Self::Arr) -> (Vec<Option<Self::Obj>>, Vec<Option<Self::Obj>>) {
        (vec![], vec![Some(()); op.0])
    }
}
