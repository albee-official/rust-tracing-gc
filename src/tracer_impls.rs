use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    rc::Rc,
    sync::Arc,
};

use super::*;

unsafe impl<T: Trace> Trace for Gc<T> {
    fn trace(&self, tracer: &mut Tracer) {
        let obj = unsafe { self.as_ptr().as_ref() };

        match tracer.traversal() {
            TraceTraversal::Deep { id } => {
                if tracer.was_last_visitor_of(self.as_ptr()) {
                    return;
                }

                obj.set_last_deep_tracer_id(*id);

                if T::needs_trace() {
                    tracer.push_next_ptr(self.as_ptr());
                }
            }

            TraceTraversal::Shallow => (),
        }

        match tracer.operation_mut() {
            TraceOperation::Mark { generation } => {
                obj.set_generation(*generation);
            }

            TraceOperation::Root => {
                self.root();
            }

            // SAFETY: unroot trace is itself unsafe
            TraceOperation::Unroot => unsafe {
                self.unroot();
            },

            TraceOperation::VisitFn { visitor } => visitor(self.as_ptr()),
        }
    }

    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }
}

unsafe impl<T: Trace> Trace for Option<T> {
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

unsafe impl<T: Trace, Error: Trace> Trace for Result<T, Error> {
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

unsafe impl<T: Trace> Trace for [T] {
    fn trace(&self, tracer: &mut Tracer) {
        if !T::needs_trace() {
            return;
        }

        for x in self {
            x.trace(tracer);
        }
    }
}

unsafe impl<T: Trace, const N: usize> Trace for [T; N] {
    fn needs_trace() -> bool {
        T::needs_trace()
    }

    fn trace(&self, tracer: &mut Tracer) {
        self.as_slice().trace(tracer)
    }
}

unsafe impl<K: Trace, V: Trace, S> Trace for HashMap<K, V, S> {
    fn needs_trace() -> bool {
        K::needs_trace() || V::needs_trace()
    }

    fn trace(&self, tracer: &mut Tracer) {
        if !Self::needs_trace() {
            return;
        }
        for (k, v) in self {
            tracer.trace(k);
            tracer.trace(v);
        }
    }
}

unsafe impl<T: Trace> Trace for RefCell<T> {
    fn needs_trace() -> bool {
        T::needs_trace()
    }

    fn trace(&self, tracer: &mut Tracer) {
        // Same policy as GcObject::trace_data: if it's mutably borrowed, its
        // contents are already rooted, so skipping is safe.

        if let Ok(inner) = self.try_borrow() {
            tracer.trace(&*inner);
        }
    }
}

unsafe impl<T: Trace> Trace for Cell<T> {
    fn needs_trace() -> bool {
        T::needs_trace()
    }

    fn trace(&self, tracer: &mut Tracer) {
        // SAFETY: tracing never calls `set`/`replace` on this cell, and the
        // reference doesn't outlive this call. (Note: a user-supplied
        // `visit_shallow` closure must not touch the cell either.)
        let inner = unsafe { &*self.as_ptr() };
        tracer.trace(inner);
    }
}

unsafe impl<T: ?Sized> Trace for Rc<T> {
    fn needs_trace() -> bool {
        false
    }
    fn trace(&self, _: &mut Tracer) {}
}

unsafe impl<T: ?Sized> Trace for Arc<T> {
    fn needs_trace() -> bool {
        false
    }
    fn trace(&self, _: &mut Tracer) {}
}

// &'static T: static data is never collected, so nothing to trace.
unsafe impl<T: ?Sized> Trace for &'static T {
    fn needs_trace() -> bool {
        false
    }
    fn trace(&self, _: &mut Tracer) {}
}

#[macro_export]
macro_rules! impl_trace_smart_ptr {
    ($($ptr:ident),* $(,)?) => {
        $(
            unsafe impl<T: Trace> Trace for $ptr<T> {
                fn needs_trace() -> bool { T::needs_trace() }
                fn trace(& self, tracer: &mut Tracer) { (**self).trace(tracer) }
            }

            unsafe impl<T: Trace> Trace for $ptr<[T]> {
                fn needs_trace() -> bool { T::needs_trace() }
                fn trace(& self, tracer: &mut Tracer) { (**self).trace(tracer) }
            }

            unsafe impl Trace for $ptr<dyn Trace> {
                fn trace(& self, tracer: &mut Tracer) { (**self).trace(tracer) }
            }
        )*
    };
}

#[macro_export]
macro_rules! impl_trace_leaf {
    ($($t:ty),* $(,)?) => {
        $(
            unsafe impl Trace for $t {
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
            unsafe impl<T: Trace> Trace for $t<T> {
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

macro_rules! impl_trace_tuple {
    ($(($($name:ident),+)),+ $(,)?) => {
        $(
            unsafe impl<$($name: Trace),+> Trace for ($($name,)+) {
                fn needs_trace() -> bool {
                    false $(|| $name::needs_trace())+
                }

                #[allow(non_snake_case)]
                fn trace(&self, tracer: &mut Tracer) {
                    let ($($name,)+) = self;
                    $( tracer.trace($name); )+
                }
            }
        )+
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

impl_trace_tuple!(
    (A),
    (A, B),
    (A, B, C),
    (A, B, C, D),
    (A, B, C, D, E),
    (A, B, C, D, E, F),
    (A, B, C, D, E, F, G),
    (A, B, C, D, E, F, G, H),
);
