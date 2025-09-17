use std::marker::PhantomData;

pub struct Value<T> {
    _phantom: PhantomData<T>,
}
