#[derive(Debug)]
pub(crate) struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
            rank: vec![0; len],
        }
    }

    pub(crate) fn find(&mut self, node: usize) -> usize {
        if self.parent[node] != node {
            self.parent[node] = self.find(self.parent[node]);
        }
        self.parent[node]
    }

    pub(crate) fn equivalent(&mut self, lhs: usize, rhs: usize) -> bool {
        self.find(lhs) == self.find(rhs)
    }

    pub(crate) fn union(&mut self, lhs: usize, rhs: usize) -> bool {
        let mut lhs = self.find(lhs);
        let mut rhs = self.find(rhs);
        if lhs == rhs {
            return false;
        }

        if self.rank[lhs] < self.rank[rhs] {
            std::mem::swap(&mut lhs, &mut rhs);
        }
        self.parent[rhs] = lhs;
        if self.rank[lhs] == self.rank[rhs] {
            self.rank[lhs] += 1;
        }
        true
    }
}
