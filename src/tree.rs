/// Values are rose trees.
/// Node labeled 'Arr', leaves labeled 'Obj'.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tree<Leaf, Node> {
    // Variable name and its type
    Leaf(usize, Leaf),
    Node(Node, Vec<Tree<Leaf, Node>>),
}

impl<Node: std::fmt::Display, Leaf: std::fmt::Display> std::fmt::Display for Tree<Leaf, Node> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tree::Leaf(i, label) => write!(f, "({i} : {label})"),
            Tree::Node(lbl, children) => {
                write!(f, "{}(", lbl)?;
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", child)?;
                }
                write!(f, ")")
            }
        }
    }
}
