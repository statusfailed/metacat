use hexpr::Operation;
use metacat::syntax::{Declaration, TheoryBundle};

pub fn find_definition<'a>(
    bundle: &'a TheoryBundle,
    name: &str,
) -> anyhow::Result<&'a Declaration> {
    let operation: Operation = name.parse()?;
    bundle
        .definitions
        .get(&operation)
        .ok_or_else(|| anyhow::anyhow!("definition '{}' not found", name))
}

pub fn find_arrow_declaration<'a>(
    bundle: &'a TheoryBundle,
    name: &str,
) -> anyhow::Result<&'a Declaration> {
    bundle
        .declarations
        .iter()
        .find(|declaration| {
            declaration.name.as_str() == name
                && matches!(declaration.theory.as_str(), "arrow" | "def-arrow")
        })
        .ok_or_else(|| anyhow::anyhow!("arrow '{}' not found", name))
}

pub fn forget_labels<T, A>(
    f: open_hypergraphs::lax::OpenHypergraph<T, A>,
) -> open_hypergraphs::lax::OpenHypergraph<(), A> {
    f.map_nodes(|_| ())
}
