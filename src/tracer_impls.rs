use std::collections::{BTreeSet, HashSet, VecDeque};

use super::*;

impl<T: Trace> Trace for Gc<T> {
    fn trace(&self, tracer: &Tracer) {
        let boxed = unsafe { self.boxed.as_ref() };

        match tracer.operation {
            TraceOperation::Mark { generation } => {
                boxed.last_tracer_id.set(tracer.id);
                boxed.generation.set(generation);

                if T::NEEDS_TRACE
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
    const NEEDS_TRACE: bool = T::NEEDS_TRACE;

    fn trace(&self, tracer: &Tracer) {
        if !T::NEEDS_TRACE {
            return;
        }

        if let Some(d) = self {
            d.trace(tracer);
        }
    }
}

impl<T: Trace, Error: Trace> Trace for Result<T, Error> {
    const NEEDS_TRACE: bool = T::NEEDS_TRACE || Error::NEEDS_TRACE;

    fn trace(&self, tracer: &Tracer) {
        match self {
            Ok(v) => v.trace(tracer),
            Err(e) => e.trace(tracer),
        }
    }
}

impl<T: Trace> Trace for [T] {
    const NEEDS_TRACE: bool = T::NEEDS_TRACE;

    fn trace(&self, tracer: &Tracer) {
        if !T::NEEDS_TRACE {
            return;
        }

        for x in self {
            x.trace(tracer);
        }
    }
}

#[macro_export]
macro_rules! impl_trace_leaf {
    ($($t:ty),* $(,)?) => {
        $( impl Trace for $t {
                const NEEDS_TRACE: bool = false;
                fn trace(&self, _: &Tracer) { }
            }
        )*
    };
}

#[macro_export]
macro_rules! impl_trace_iter {
    ($($t:ident),* $(,)?) => {
        $(
            impl<T: Trace> Trace for $t<T> {
                const NEEDS_TRACE: bool = T::NEEDS_TRACE;
                fn trace(&self, tracer: &Tracer) {
                    if !T::NEEDS_TRACE {
                        return;
                    }

                    for x in self { x.trace(tracer); }
                }
            }
        )*
    };
}

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
