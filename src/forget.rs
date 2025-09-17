//! OpenHypergraph::forget, but modified to only forget 1 -> 1 'identity' copies
use open_hypergraphs::category::*;
use open_hypergraphs::finite_function::FiniteFunction;
use open_hypergraphs::lax::functor::*;
use open_hypergraphs::lax::var::*;
use open_hypergraphs::lax::*;

#[derive(Clone)]
pub struct ForgetMonogamous;

pub fn forget_monogamous<
    O: Clone + PartialEq + std::fmt::Debug,
    A: Clone + PartialEq + HasVar + std::fmt::Debug,
>(
    f: &OpenHypergraph<O, A>,
) -> OpenHypergraph<O, A> {
    ForgetMonogamous.map_arrow(f)
}

impl<O: Clone + PartialEq + std::fmt::Debug, A: HasVar + Clone + PartialEq + std::fmt::Debug>
    Functor<O, A, O, A> for ForgetMonogamous
{
    // Identity-on-objects
    fn map_object(&self, o: &O) -> impl ExactSizeIterator<Item = O> {
        std::iter::once(o.clone())
    }

    fn map_operation(&self, a: &A, source: &[O], target: &[O]) -> OpenHypergraph<O, A> {
        if source.len() != 1 || target.len() != 1 {
            return OpenHypergraph::singleton(a.clone(), source.to_vec(), target.to_vec());
        }

        // Eliminate var-labeled operations which have all their sources + targets the same type.
        if *a == HasVar::var() && all_elements_equal(source, target) {
            // Extra-special frobenius axiom: 0 → 0 copy is the empty diagram
            if source.is_empty() && target.is_empty() {
                return OpenHypergraph::empty();
            }

            // At least one must have a value
            let label = {
                if source.is_empty() {
                    target[0].clone()
                } else {
                    source[0].clone()
                }
            };

            let s = FiniteFunction::terminal(source.len());
            let t = FiniteFunction::terminal(target.len());

            return OpenHypergraph::<O, A>::spider(s, t, vec![label]).unwrap();
        }
        OpenHypergraph::singleton(a.clone(), source.to_vec(), target.to_vec())
    }

    fn map_arrow(&self, f: &OpenHypergraph<O, A>) -> OpenHypergraph<O, A> {
        define_map_arrow(self, f)
    }
}

// Are all elements of both lists equal? (not pairwise- equivalent to reduce(equal, concat(s, t)))
fn all_elements_equal<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a.iter()
        .chain(b.iter())
        .all(|x| *x == *a.first().unwrap_or(x))
}
