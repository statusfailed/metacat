use hexpr::*;
use metacat::prop::{Nat, PropObj};
use metacat::theory::{OperationKey, Theory};
use std::collections::HashMap;
use std::path::PathBuf;

/// A complete parsed metacat file containing theories and definitions
pub struct TheoryBundle {
    #[allow(unused)]
    pub hexprs: Vec<Hexpr>,
    pub object_theory: Theory<Nat>,
    pub arrow_theory: Theory<OperationKey>, // contains the types of both arrows *and* defined arrows
    // def-arrow maps have their bodies (rhs of =) here.
    pub definitions: HashMap<Operation, Declaration>,
}

/// A declaration is matched from hexprs of the form
/// `(<theory> <name> : <src> -> <target> = <definition>)`
/// where the `= <definition>` part is optional.
pub struct Declaration {
    pub name: Operation,
    pub source_map: Hexpr,
    pub target_map: Hexpr,
    pub definition: Option<Hexpr>,
}

impl TheoryBundle {
    /// Load a TheoryBundle from a text string
    pub fn from_text(text: &str) -> anyhow::Result<Self> {
        let hexprs: Vec<Hexpr> = parse_hexprs(text)?;

        log::info!("loading hexprs...");
        for hexpr in hexprs.iter() {
            log::info!("load {}", hexpr);
        }

        // "object theory" morphisms *logical symbols* and *terms* of the theory
        let object_theory = read_theory(&PropObj, "object", &hexprs)?;

        // "arrow theory" morphisms are the *axioms* and *proofs*
        let arrow_theory = read_theory(&object_theory, "arrow", &hexprs)?;

        // Load definitions
        let mut definitions = HashMap::new();
        for def in read_definitions("def-arrow", &hexprs) {
            if def.definition.is_some() {
                definitions.insert(def.name.clone(), def);
            }
        }

        Ok(TheoryBundle {
            hexprs,
            object_theory,
            arrow_theory,
            definitions,
        })
    }

    /// Load a TheoryBundle from a file
    pub fn from_file(path: PathBuf) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_text(&text)
    }
}

/// read a theory from a list of hexprs
fn read_theory<S: Signature<Obj = ()>>(
    signature: &S,
    declaration_literal: &str,
    hexprs: &Vec<Hexpr>,
) -> anyhow::Result<Theory<S::Arr>>
where
    S::Arr: Clone,
    S::Error: Sync + Send + std::error::Error + 'static,
{
    let mut theory = Theory::new();
    log::info!("reading {} theory", declaration_literal);
    for hexpr in hexprs {
        log::debug!("load {}", hexpr);
        if let Some(decl) = Declaration::try_from_hexpr(hexpr, declaration_literal) {
            log::trace!("converting source map");
            let source = forget_labels(try_interpret(signature, &decl.source_map)?);
            log::trace!("converting target map");
            let target = forget_labels(try_interpret(signature, &decl.target_map)?);
            log::trace!("addding operation");
            theory.add_operation(decl.name, source, target)?;
        }
    }

    Ok(theory)
}

/// Read a set of declarations from a list of hexprs
fn read_definitions(declaration_literal: &str, hexprs: &Vec<Hexpr>) -> Vec<Declaration> {
    hexprs
        .iter()
        .filter_map(|hexpr| Declaration::try_from_hexpr(hexpr, declaration_literal))
        .filter(|decl| decl.definition.is_some())
        .collect()
}

impl Declaration {
    /// Try and match a hexpr of the form
    /// `(<theory> <name> : <src> -> <target> = <definition>)`
    fn try_from_hexpr(hexpr: &Hexpr, declaration_literal: &str) -> Option<Declaration> {
        let Hexpr::Composition(parts) = hexpr else {
            return None;
        };

        let (name, source, target, def) = match &parts[..] {
            [lit, name, colon, source, arrow, target]
                if is_operation(lit, declaration_literal)
                    && is_operation(colon, ":")
                    && is_operation(arrow, "->") =>
            {
                (name, source, target, None)
            }
            [lit, name, colon, source, arrow, target, eq, def]
                if is_operation(lit, declaration_literal)
                    && is_operation(colon, ":")
                    && is_operation(arrow, "->")
                    && is_operation(eq, "=") =>
            {
                (name, source, target, Some(def))
            }
            _ => return None,
        };

        let Hexpr::Operation(name) = name else {
            return None;
        };

        Some(Declaration {
            name: name.clone(),
            source_map: source.clone(),
            target_map: target.clone(),
            definition: def.cloned(),
        })
    }
}

fn is_operation(hexpr: &Hexpr, literal: &str) -> bool {
    match hexpr {
        Hexpr::Operation(op) => op.as_str() == literal,
        _ => false,
    }
}

fn forget_labels<T, A>(
    f: open_hypergraphs::lax::OpenHypergraph<T, A>,
) -> open_hypergraphs::lax::OpenHypergraph<(), A> {
    f.map_nodes(|_| ())
}
