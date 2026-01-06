use hexpr::*;
use open_hypergraphs::lax::OpenHypergraph;

use metacat::check::eval_type;
use metacat::check::to_type_map;
use metacat::prop::*;
use metacat::theory::*;

// CLI utils
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// A declaration is matched from hexprs of the form
/// `(<theory> <name> : <src> -> <target> = <definition>)`
/// where the `= <definition>` part is optional.
struct Declaration {
    name: Operation,
    source_map: Hexpr,
    target_map: Hexpr,
    definition: Option<Hexpr>,
}

#[derive(Parser)]
#[command(name = "metacat-cli")]
#[command(about = "A tool for checking categorical definitions")]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Check {
        #[arg()]
        path: PathBuf,
    },
    Arrow {
        #[arg()]
        path: PathBuf,
        #[arg()]
        name: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize logger based on verbosity level
    let log_level = match cli.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    env_logger::Builder::new().filter_level(log_level).init();

    match cli.command {
        Command::Check { path } => check(path),
        Command::Arrow { path, name } => arrow(path, name),
    }
}

/// Load theories and hexprs from a file
fn load_theories(path: PathBuf) -> anyhow::Result<(Vec<Hexpr>, Theory<Nat>, Theory<OperationKey>)> {
    let text = std::fs::read_to_string(path)?;
    let hexprs: Vec<Hexpr> = parse_hexprs(&text)?;

    log::info!("got hexprs:");
    for hexpr in hexprs.iter() {
        log::info!("{}", hexpr);
    }

    // "object theory" morphisms *logical symbols* and *terms* of the theory
    let object_theory = read_theory(&PropObj, "object", &hexprs)?;

    // "arrow theory" morphisms are the *axioms* and *proofs*
    let arrow_theory = read_theory(&object_theory, "arrow", &hexprs)?;

    Ok((hexprs, object_theory, arrow_theory))
}

/// Read a file of `Declaration`s into object and arrow theories,
/// then check all definitions.
fn check(path: PathBuf) -> anyhow::Result<()> {
    let (hexprs, object_theory, arrow_theory) = load_theories(path)?;

    for def in read_definitions("def-arrow", &hexprs) {
        // TODO: remove unwrap
        let def_hexpr = def.definition.unwrap();
        log::info!(
            "checking definition {} : {} -> {} = {}",
            def.name,
            def.source_map,
            def.target_map,
            def_hexpr
        );

        // NOTE: we use forget_labels instead of unify, since we have a single-sorted theory.
        let term = forget_labels(try_interpret(&arrow_theory, &def_hexpr)?);
        let source = forget_labels(try_interpret(&object_theory, &def.source_map)?);
        let target = forget_labels(try_interpret(&object_theory, &def.target_map)?);

        let type_term = to_type_map(arrow_theory.clone(), source, target, &term);
        let result = eval_type(type_term);
        log::debug!("eval_type: {:?}", result);

        match result {
            Ok(_) => {
                println!("✅ {} : {} -> {}", def.name, def.source_map, def.target_map);
            }
            Err(e) => {
                println!("❌ {} : {} -> {}", def.name, def.source_map, def.target_map);
                println!("   Error: {}", e);
            }
        }
    }

    Ok(())
}

/// Load theories from a file and print the hexpr for a given arrow name
fn arrow(path: PathBuf, name: String) -> anyhow::Result<()> {
    log::info!("Loading theories to find arrow: {}", name);
    let (hexprs, _object_theory, _arrow_theory) = load_theories(path)?;

    // NOTE: operation_key is guaranteed to be in the theory; try_parse_op will fail otherwise.
    //let operation_key = arrow_theory.try_parse_op(&name.parse()?)?;
    // Look for a definition with the given name

    // TODO: load_theories should make a hashmap of name → definition
    let definitions = read_definitions("def-arrow", &hexprs);
    for def in definitions {
        if def.name.as_str() == name {
            if let Some(def_hexpr) = def.definition {
                println!("{}", def_hexpr);
                return Ok(());
            }
        }
    }

    Ok(())
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
    for hexpr in hexprs {
        if let Some(decl) = Declaration::try_from_hexpr(hexpr, declaration_literal) {
            let source = unify(try_interpret(signature, &decl.source_map)?)?;
            let target = unify(try_interpret(signature, &decl.target_map)?)?;
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
