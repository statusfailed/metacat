use hexpr::*;

fn main() -> anyhow::Result<()> {
    let hexpr: Hexpr = "(foo bar)".parse()?;
    println!("{}", hexpr);
    Ok(())
}
