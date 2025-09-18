use crate::lang::{Arr, Biprofile, Builder, Obj, Var};
use crate::tree::Tree;
use open_hypergraphs::lax::var;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FOL {
    Provable,
    Forall,
    Exists,
    Not,
    Implies,
    Equiv,
    And,
    Or,
    True,
    Phi, // TODO: remove me!
}

impl std::fmt::Display for FOL {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FOL::Provable => write!(f, "⊢"),
            FOL::Forall => write!(f, "∀ "),
            FOL::Exists => write!(f, "∃ "),
            FOL::Not => write!(f, "¬"),
            FOL::Implies => write!(f, "→"),
            FOL::Equiv => write!(f, "↔"),
            FOL::And => write!(f, "∧"),
            FOL::Or => write!(f, "∨"),
            FOL::True => write!(f, "⊤"),
            FOL::Phi => write!(f, "𝜑"),
        }
    }
}

/// Convert a number to subscript Unicode characters
pub fn to_subscript(n: usize) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            _ => c, // fallback for non-digits
        })
        .collect()
}

/// Pretty-print a FOL term (Tree<Obj, FOL>) using infix notation where appropriate
pub fn pretty_print_fol(term: &Tree<Obj, FOL>) -> String {
    match term {
        Tree::Leaf(i, _) => format!("𝑥{}", to_subscript(*i)),
        Tree::Node(op, children) => {
            match (op, children.len()) {
                // Binary infix operators
                (FOL::Implies, 2) => format!(
                    "({} → {})",
                    pretty_print_fol(&children[0]),
                    pretty_print_fol(&children[1])
                ),
                (FOL::Equiv, 2) => format!(
                    "({} ↔ {})",
                    pretty_print_fol(&children[0]),
                    pretty_print_fol(&children[1])
                ),
                (FOL::And, 2) => format!(
                    "({} ∧ {})",
                    pretty_print_fol(&children[0]),
                    pretty_print_fol(&children[1])
                ),
                (FOL::Or, 2) => format!(
                    "({} ∨ {})",
                    pretty_print_fol(&children[0]),
                    pretty_print_fol(&children[1])
                ),

                // Unary prefix operators
                (FOL::Not, 1) => format!("¬{}", pretty_print_fol(&children[0])),
                (FOL::Provable, 1) => format!("⊢ {}", pretty_print_fol(&children[0])),

                // Quantifiers (special case with variable)
                (FOL::Forall, 2) => format!(
                    "∀{}.{}",
                    pretty_print_fol(&children[1]),
                    pretty_print_fol(&children[0])
                ),
                (FOL::Exists, 2) => format!(
                    "∃{}.{}",
                    pretty_print_fol(&children[1]),
                    pretty_print_fol(&children[0])
                ),

                // Phi with arguments
                (FOL::Phi, 2) => format!(
                    "φ({}, {})",
                    pretty_print_fol(&children[0]),
                    pretty_print_fol(&children[1])
                ),

                // Constants
                (FOL::True, 0) => "⊤".to_string(),

                // Fallback to prefix notation
                _ => {
                    let args: Vec<String> = children
                        .iter()
                        .map(|child| pretty_print_fol(child))
                        .collect();
                    format!("{}({})", op, args.join(", "))
                }
            }
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
            FOL::Equiv => (2, 1),
            FOL::And => (2, 1),
            FOL::Or => (2, 1),
            FOL::True => (0, 1),
            FOL::Phi => (2, 1),
        }
    }

    pub fn call(&self, builder: &Builder<FOL>, args: Vec<Var<FOL>>) -> Var<FOL> {
        var::fn_operation(builder, &args, Obj, Arr::Fwd(self.clone()))
    }
}
