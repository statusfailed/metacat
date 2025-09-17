use crate::lang::{Arr, Biprofile, Builder, Var};
use open_hypergraphs::lax::var;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FOL {
    Provable,
    Forall,
    Exists,
    Not,
    Implies,
    And,
    Or,
    Phi, // TODO: remove me!
}

impl std::fmt::Display for FOL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FOL::Provable => write!(f, "⊢"),
            FOL::Forall => write!(f, "∀"),
            FOL::Exists => write!(f, "∃"),
            FOL::Not => write!(f, "¬"),
            FOL::Implies => write!(f, "→"),
            FOL::And => write!(f, "∧"),
            FOL::Or => write!(f, "∨"),
            FOL::Phi => write!(f, "φ"),
        }
    }
}

impl FOL {
    pub fn biprofile(&self) -> Biprofile {
        match self {
            FOL::Provable => (1, 1),
            FOL::Forall => (2, 1),
            FOL::Exists => (2, 1),
            FOL::Not => (1, 1),
            FOL::Implies => (2, 1),
            FOL::And => (2, 1),
            FOL::Or => (2, 1),
            FOL::Phi => (2, 1),
        }
    }

    pub fn call(&self, builder: &Builder<FOL>, args: Vec<Var<FOL>>) -> Var<FOL> {
        var::fn_operation(builder, &args, (), Arr::Fwd(self.clone()))
    }
}
