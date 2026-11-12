use std::collections::{BTreeSet, HashSet, VecDeque};

use super::*;

impl<T: Trace> Trace for Gc<T> {
    fn trace(&self, tracer: &mut Tracer) {
        let ptr = unsafe { self.ptr.as_ref() };

        match tracer.traversal {
            TraceTraversal::Deep { id } => {
                if tracer.was_last_visitor_of(self.ptr) {
                    return;
                }

                ptr.last_deep_tracer_id.set(id);

                if T::needs_trace() {
                    tracer.queue_next_ptr(self.ptr);
                }
            }

            TraceTraversal::Shallow => (),
        }

        match tracer.operation {
            TraceOperation::Mark { generation } => {
                ptr.generation.set(generation);
            }

            TraceOperation::Root => {
                self.root_cell.root(self.ptr);
            }

            TraceOperation::Unroot => {
                self.root_cell.unroot();
            }
        }
    }
}

impl<T: Trace> Trace for Option<T> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        T::needs_trace()
    }

    fn trace(&self, tracer: &mut Tracer) {
        if !T::needs_trace() {
            return;
        }

        if let Some(d) = self {
            d.trace(tracer);
        }
    }
}

impl<T: Trace, Error: Trace> Trace for Result<T, Error> {
    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        T::needs_trace() || Error::needs_trace()
    }

    fn trace(&self, tracer: &mut Tracer) {
        match self {
            Ok(v) => v.trace(tracer),
            Err(e) => e.trace(tracer),
        }
    }
}

impl<T: Trace> Trace for [T] {
    fn trace(&self, tracer: &mut Tracer) {
        if !T::needs_trace() {
            return;
        }

        for x in self {
            x.trace(tracer);
        }
    }
}

impl<T: Trace, const N: usize> Trace for [T; N] {
    fn needs_trace() -> bool {
        T::needs_trace()
    }

    fn trace(&self, tracer: &mut Tracer) {
        self.as_slice().trace(tracer)
    }
}

#[macro_export]
macro_rules! impl_trace_smart_ptr {
    ($($ptr:ident),* $(,)?) => {
        $(
            impl<T: Trace> Trace for $ptr<T> {
                fn needs_trace() -> bool { T::needs_trace() }
                fn trace(& self, tracer: &mut Tracer) { (**self).trace(tracer) }
            }

            impl<T: Trace> Trace for $ptr<[T]> {
                fn needs_trace() -> bool { T::needs_trace() }
                fn trace(& self, tracer: &mut Tracer) { (**self).trace(tracer) }
            }

            impl Trace for $ptr<dyn Trace> {
                fn trace(& self, tracer: &mut Tracer) { (**self).trace(tracer) }
            }
        )*
    };
}

#[macro_export]
macro_rules! impl_trace_leaf {
    ($($t:ty),* $(,)?) => {
        $(
            impl Trace for $t {
                fn needs_trace() -> bool
                where
                    Self: Sized,
                {
                    false
                }

                fn trace(&self, _: &mut Tracer) {}
            }
        )*
    };
}

#[macro_export]
macro_rules! impl_trace_iter {
    ($($t:ident),* $(,)?) => {
        $(
            impl<T: Trace> Trace for $t<T> {
                fn needs_trace() -> bool
                where
                    Self: Sized,
                {
                    T::needs_trace()
                }

                fn trace(& self, tracer: &mut Tracer) {
                    if !T::needs_trace() {
                        return;
                    }

                    for x in self {
                        x.trace(tracer);
                    }
                }
            }
        )*
    };
}

impl_trace_smart_ptr!(Box);

impl_trace_iter!(Vec, VecDeque, HashSet, BTreeSet);

impl_trace_leaf!(
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    f32,
    f64,
    bool,
    char,
    String,
    (),
);
