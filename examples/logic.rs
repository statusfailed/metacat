use hexpr::*;

use metacat::check::eval_type;
use metacat::check::to_type_map;
use metacat::prop::*;
use metacat::theory::*;

use open_hypergraphs_dot::{Options, svg::to_svg_with};

fn is_operation(hexpr: &Hexpr, literal: &str) -> bool {
    match hexpr {
        Hexpr::Operation(op) => op.as_str() == literal,
        _ => false,
    }
}

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
        if let Hexpr::Composition(hexprs) = hexpr {
            if let [obj_lit, name, colon, source_map, arrow, target_map] = &hexprs[..] {
                if !is_operation(obj_lit, declaration_literal)
                    || !is_operation(colon, ":")
                    || !is_operation(arrow, "->")
                {
                    continue;
                }

                let Hexpr::Operation(name) = name else {
                    continue;
                };

                let source = unify(try_interpret(signature, &source_map)?)?;
                let target = unify(try_interpret(signature, &target_map)?)?;
                theory.add_operation(name.clone(), source, target)?
            };
        }
    }

    Ok(theory)
}

fn main() -> anyhow::Result<()> {
    let text = std::fs::read_to_string("fol.hex")?;
    let hexprs: Vec<Hexpr> = parse_hexprs(&text)?;

    println!("got hexprs:");
    for hexpr in hexprs.iter() {
        println!("{}", hexpr);
    }

    let object_theory = read_theory(&PropObj, "object", &hexprs)?;
    let arrow_theory = read_theory(&object_theory, "arrow", &hexprs)?;

    let term = unify(try_interpret(&arrow_theory, &"(wi wn)".parse()?)?)?;

    let source = unify(try_interpret(&object_theory, &"{wff wff}".parse()?)?)?;
    let target = unify(try_interpret(&object_theory, &"(-> -. wff)".parse()?)?)?;

    // TODO: PERFORMANCE: theory is cloned *twice* - incredibly wasteful!
    let term = to_type_map(arrow_theory.clone(), source, target, &term);

    std::fs::write(
        "out.svg",
        to_svg_with(
            &term.clone().map_nodes(|_| ""),
            &Options::default().display().tb(),
        )?,
    )?;

    let result = eval_type(term);
    println!("eval_type: {:?}", result);

    Ok(())
}
