use hexpr::*;
use metacat::prop::{Nat, PropObj};
use metacat::theory::{OperationKey, Theory};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("Invalid declaration: {0}")]
    InvalidDeclaration(Hexpr),
    #[error("Theory error: {0}")]
    TheoryError(#[from] metacat::theory::Error),
    #[error("Hexpr conversion error: {0}")]
    InterpretError(#[from] hexpr::interpret::Error<metacat::theory::Error>),
    #[error("PropObj interpret error: {0}")]
    PropObjInterpretError(#[from] hexpr::interpret::Error<std::num::ParseIntError>),
    #[error("Parse error: {0}")]
    ParseError(#[from] hexpr::ParseError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// A complete parsed metacat file containing theories and definitions
pub struct TheoryBundle {
    pub object_theory: Theory<Nat>,
    pub arrow_theory: Theory<OperationKey>, // contains the types of both arrows *and* defined arrows
    // def-arrow maps have their bodies (rhs of =) here.
    pub definitions: HashMap<Operation, Declaration>,
}

/// A declaration is matched from hexprs of the form
/// `(<theory> <name> : <src> -> <target> = <definition>)`
/// where the `= <definition>` part is optional.
pub struct Declaration {
    pub theory: Operation,
    pub name: Operation,
    pub source_map: Hexpr,
    pub target_map: Hexpr,
    pub definition: Option<Hexpr>,
}

impl TheoryBundle {
    /// Load a TheoryBundle from a text string
    pub fn from_text(text: &str) -> Result<Self, LoadError> {
        let hexprs: Vec<Hexpr> = parse_hexprs(text)?;

        let declarations: Vec<Declaration> = hexprs
            .into_iter()
            .map(|hexpr| {
                Declaration::try_from_hexpr(hexpr.clone())
                    .ok_or(LoadError::InvalidDeclaration(hexpr))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Self::from_declarations(declarations)
    }

    fn from_declarations(decls: Vec<Declaration>) -> Result<Self, LoadError> {
        let object_theory = read_object_theory(&decls)?;
        let arrow_theory = read_arrow_theory(&object_theory, &decls)?;

        // Collect definitions
        let mut definitions = HashMap::new();
        for decl in decls {
            if decl.definition.is_some() && decl.theory.as_str() == "def-arrow" {
                definitions.insert(decl.name.clone(), decl);
            }
        }

        Ok(Self {
            object_theory,
            arrow_theory,
            definitions,
        })
    }

    /// Load a TheoryBundle from a file
    pub fn from_file(path: PathBuf) -> Result<Self, LoadError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_text(&text)
    }
}

// TODO: avoid unnecessary cloning in here; it takes ownership!
impl Declaration {
    /// Try and match a hexpr of the form
    /// `(<theory> <name> : <src> -> <target> = <definition>)`
    fn try_from_hexpr(hexpr: Hexpr) -> Option<Declaration> {
        let Hexpr::Composition(parts) = hexpr else {
            return None;
        };

        let (lit, name, source, target, def) = match &parts[..] {
            [lit, name, colon, source, arrow, target]
                if is_operation(colon, ":") && is_operation(arrow, "->") =>
            {
                (lit, name, source, target, None)
            }
            [lit, name, colon, source, arrow, target, eq, def]
                if is_operation(colon, ":")
                    && is_operation(arrow, "->")
                    && is_operation(eq, "=") =>
            {
                (lit, name, source, target, Some(def))
            }
            _ => return None,
        };

        let Hexpr::Operation(name) = name else {
            return None;
        };

        let Hexpr::Operation(theory) = lit else {
            return None;
        };

        // Validate theory name
        match theory.as_str() {
            "object" | "arrow" | "def-arrow" => {}
            _ => return None,
        }

        Some(Declaration {
            theory: theory.clone(),
            name: name.clone(),
            source_map: source.clone(),
            target_map: target.clone(),
            definition: def.cloned(),
        })
    }
}

/// read object theory from declarations
fn read_object_theory(declarations: &[Declaration]) -> Result<Theory<Nat>, LoadError> {
    let mut theory = Theory::new();
    log::info!("reading object theory");
    for decl in declarations {
        if decl.theory.as_str() != "object" {
            continue;
        }
        log::debug!("load {}", &decl.name);
        let source = forget_labels(try_interpret(&PropObj, &decl.source_map)?);
        let target = forget_labels(try_interpret(&PropObj, &decl.target_map)?);
        theory.add_operation(decl.name.clone(), source, target)?;
    }
    Ok(theory)
}

/// read arrow theory from declarations
fn read_arrow_theory(
    object_theory: &Theory<Nat>,
    declarations: &[Declaration],
) -> Result<Theory<OperationKey>, LoadError> {
    let mut theory = Theory::new();
    log::info!("reading arrow theory");
    for decl in declarations {
        if decl.theory.as_str() != "arrow" && decl.theory.as_str() != "def-arrow" {
            continue;
        }
        log::debug!("load {}", &decl.name);
        let source = forget_labels(try_interpret(object_theory, &decl.source_map)?);
        let target = forget_labels(try_interpret(object_theory, &decl.target_map)?);
        theory.add_operation(decl.name.clone(), source, target)?;
    }
    Ok(theory)
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
