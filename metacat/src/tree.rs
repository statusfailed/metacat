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
                        if self.is_infix() {
                            Ok(format!(
                                "{} {op} {}",
                                children[0].try_pretty_operand(coarity)?,
                                children[1].try_pretty_operand(coarity)?
                            ))
                        } else {
                            Ok(format!(
                                "{op}({}, {})",
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

    fn is_infix(&self) -> bool {
        match self {
            Tree::Node(op, _, children) if children.len() == 2 => {
                !format!("{op}").starts_with(|c: char| c.is_alphanumeric())
            }
            _ => false,
        }
    }

    fn try_pretty_operand<E>(
        &self,
        coarity: Option<&dyn Fn(&Node) -> Result<usize, E>>,
    ) -> Result<String, E> {
        let rendered = self.try_pretty(coarity)?;
        if self.is_infix() {
            Ok(format!("({rendered})"))
        } else {
            Ok(rendered)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Tree;

    #[test]
    fn parenthesizes_nested_infix_operands() {
        let tree = Tree::<(), &str>::Node(
            "*",
            0,
            vec![
                Tree::Node("+", 0, vec![Tree::Node("1", 0, vec![]), Tree::Node("1", 0, vec![])]),
                Tree::Node("2", 0, vec![]),
            ],
        );

        let coarity = |_op: &&str| -> Result<usize, ()> { Ok(1) };
        assert_eq!(tree.try_pretty(Some(&coarity)).unwrap(), "(1 + 1) * 2");
    }
}
