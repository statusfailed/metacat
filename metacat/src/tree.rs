/// Values are rose trees.
/// Node labeled 'Arr', leaves labeled 'Obj'.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tree<Leaf, Node> {
    // Variable name and its type
    Empty,
    Leaf(usize, Leaf),
    Node(Node, usize, Vec<Tree<Leaf, Node>>),
}

impl<Leaf, Node: std::fmt::Display> Tree<Leaf, Node> {
    pub fn try_pretty<E>(
        &self,
        coarity: Option<&dyn Fn(&Node) -> Result<usize, E>>,
    ) -> Result<String, E> {
        match self {
            Tree::Empty => Ok("empty".to_string()),
            Tree::Leaf(i, _) => Ok(format!("x{i}")),
            Tree::Node(op, target_idx, children) => {
                let inner = match children.len() {
                    0 => Ok(format!("{op}")),
                    1 => Ok(format!("{op}({})", children[0].try_pretty(coarity)?)),
                    2 => {
                        let op_str = format!("{op}");
                        if op_str.starts_with(|c: char| c.is_alphanumeric()) {
                            Ok(format!(
                                "{op}({}, {})",
                                children[0].try_pretty(coarity)?,
                                children[1].try_pretty(coarity)?
                            ))
                        } else {
                            Ok(format!(
                                "{} {op} {}",
                                children[0].try_pretty(coarity)?,
                                children[1].try_pretty(coarity)?
                            ))
                        }
                    }
                    _ => {
                        let args: Vec<String> = children
                            .iter()
                            .map(|c| c.try_pretty(coarity))
                            .collect::<Result<_, _>>()?;
                        Ok(format!("{op}({})", args.join(", ")))
                    }
                }?;
                let show_proj = match coarity {
                    Some(f) => f(op)? > 1,
                    None => true,
                };
                if show_proj {
                    Ok(format!("π{target_idx}({inner})"))
                } else {
                    Ok(inner)
                }
            }
        }
    }
}
