use open_hypergraphs::lax::OpenHypergraph;
use open_hypergraphs_dot::{Orientation, generate_dot_with};
use std::fmt::{Debug, Display};

pub fn to_svg<O: PartialEq + Clone + Display + Debug, A: PartialEq + Clone + Display + Debug>(
    term: &OpenHypergraph<O, A>,
) -> Result<Vec<u8>, std::io::Error> {
    use graphviz_rust::{
        cmd::{CommandArg, Format},
        exec,
        printer::PrinterContext,
    };

    let opts = open_hypergraphs_dot::Options {
        node_label: Box::new(|n| format!("{n}")),
        edge_label: Box::new(|e| format!("{e}")),
        orientation: Orientation::LR,
        ..Default::default()
    };

    let dot_graph = generate_dot_with(term, &opts);

    exec(
        dot_graph,
        &mut PrinterContext::default(),
        vec![CommandArg::Format(Format::Svg)],
    )
}

pub fn save_svg<
    O: PartialEq + Clone + std::fmt::Display + std::fmt::Debug,
    A: PartialEq + Clone + std::fmt::Display + std::fmt::Debug,
>(
    term: &open_hypergraphs::lax::OpenHypergraph<O, A>,
    filename: &str,
) -> Result<(), std::io::Error> {
    use crate::svg::to_svg;
    let bytes = to_svg(term)?;
    let output_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("images");
    std::fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(filename);
    println!("saving svg to {output_path:?}");
    std::fs::write(output_path, bytes).expect("write diagram file");
    Ok(())
}
