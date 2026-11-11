use std::collections::{BTreeSet, HashSet, VecDeque};

use super::*;

impl<T: Trace> Trace for Gc<T> {
    fn trace(&self, tracer: &Tracer) {
        let boxed = unsafe { self.boxed.as_ref() };

        match tracer.operation {
            TraceOperation::Mark { generation } => {
                // do not visit other gc objects if already visited them
                if self.traced(tracer) {
                    return;
                }

                boxed.last_tracer_id.set(tracer.id);
                boxed.generation.set(generation);

                if T::needs_trace()
                    && let Ok(data) = boxed.data.try_borrow()
                {
                    data.trace(tracer);
                }
            }

            TraceOperation::RootImmediateChildren => {
                if !self.rooted.replace(true) {
                    boxed.inc_roots()
                }
            }

            TraceOperation::UnrootImmediateChildren => {
                if self.rooted.replace(false) {
                    boxed.dec_roots()
                }
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

    fn trace(&self, tracer: &Tracer) {
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

    fn trace(&self, tracer: &Tracer) {
        match self {
            Ok(v) => v.trace(tracer),
            Err(e) => e.trace(tracer),
        }
    }
}

impl<T: Trace> Trace for [T] {
    fn trace(&self, tracer: &Tracer) {
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

    fn trace(&self, tracer: &Tracer) {
        self.as_slice().trace(tracer)
    }
}

#[macro_export]
macro_rules! impl_trace_smart_ptr {
    ($($ptr:ident),* $(,)?) => {
        $(
            impl<T: Trace> Trace for $ptr<T> {
                fn needs_trace() -> bool { T::needs_trace() }
                fn trace(&self, tracer: &Tracer) { (**self).trace(tracer) }
            }

            impl<T: Trace> Trace for $ptr<[T]> {
                fn needs_trace() -> bool { T::needs_trace() }
                fn trace(&self, tracer: &Tracer) { (**self).trace(tracer) }
            }

            impl Trace for $ptr<dyn Trace> {
                fn trace(&self, tracer: &Tracer) { (**self).trace(tracer) }
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

                fn trace(&self, _: &Tracer) {}
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

                fn trace(&self, tracer: &Tracer) {
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
