//! Machine-readable validation for translate_metamath.py's generated theories.
//!
//! Each declaration is checked independently here. The translator subsequently
//! removes failures and their dependent declarations; a failed theorem is never
//! promoted to an axiom. No custom Cargo example registration is needed.

use metacat::dv::dv_check;
use metacat::theory::{Theory, TheoryId, TheorySet};
use std::io::{self, Write};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("expected a Hex file")?;
    let start = Instant::now();
    eprintln!("Loading {path}");
    let theories = TheorySet::from_files([path.into()])?;
    let theory = &theories.theories[&TheoryId("set.proof".parse()?)];
    let Theory::Theory { arrows, .. } = theory else {
        return Err("expected set.proof".into());
    };
    eprintln!(
        "Loaded {} declarations in {:.1}s",
        arrows.len(),
        start.elapsed().as_secs_f64()
    );
    let mut output = io::BufWriter::new(io::stdout().lock());
    let mut failures = 0;
    for (index, (name, arrow)) in arrows.iter().enumerate() {
        let mut proof = match &arrow.definition {
            Some(proof) => proof.clone(),
            None => hexpr::try_interpret(&theory.local_signature(), &name.as_str().parse()?)?
                .map_nodes(|_| ()),
        };
        match dv_check(
            theory,
            arrow.type_maps.0.clone(),
            arrow.type_maps.1.clone(),
            arrow.ar.as_ref(),
            &mut proof,
        ) {
            Ok(result) => writeln!(output, "OK\t{name}\t{}", result.saturation.target)?,
            Err(error) => {
                failures += 1;
                writeln!(
                    output,
                    "FAIL\t{name}\t{}",
                    error.to_string().replace(['\n', '\t'], " ")
                )?;
            }
        }
        output.flush()?;
        if (index + 1) % 1000 == 0 {
            eprintln!(
                "Checked {} / {}, {} failures, {:.1}s",
                index + 1,
                arrows.len(),
                failures,
                start.elapsed().as_secs_f64()
            );
        }
    }
    eprintln!(
        "Checked all {} declarations: {failures} failures, {:.1}s",
        arrows.len(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
