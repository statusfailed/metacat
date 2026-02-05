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
    pub fn pretty(&self, coarity: Option<&dyn Fn(&Node) -> usize>) -> String {
        match self {
            Tree::Empty => "empty".to_string(),
            Tree::Leaf(i, _) => format!("x{i}"),
            Tree::Node(op, target_idx, children) => {
                let inner = match children.len() {
                    0 => format!("{op}"),
                    1 => format!("{op}({})", children[0].pretty(coarity)),
                    2 => {
                        let op_str = format!("{op}");
                        if op_str.starts_with(|c: char| c.is_alphanumeric()) {
                            format!("{op}({}, {})", children[0].pretty(coarity), children[1].pretty(coarity))
                        } else {
                            format!("({} {op} {})", children[0].pretty(coarity), children[1].pretty(coarity))
                        }
                    }
                    _ => {
                        let args: Vec<String> =
                            children.iter().map(|c| c.pretty(coarity)).collect();
                        format!("{op}({})", args.join(", "))
                    }
                };
                let show_proj = match coarity {
                    Some(f) => f(op) > 1,
                    None => true,
                };
                if show_proj {
                    format!("π{target_idx}({inner})")
                } else {
                    inner
                }
            }
        }
    }
}

impl<Leaf, Node: std::fmt::Display> std::fmt::Display for Tree<Leaf, Node> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pretty(None))
    }
}
