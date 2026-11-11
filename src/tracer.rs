use std::cell::Cell;

use super::*;

pub(crate) type GcTracerId = u16;

thread_local! {
    static UUID_SOURCE: Cell<GcTracerId> = Cell::from(1);
}

#[derive(Clone, Copy, Debug)]
pub enum TraceOperation {
    Mark { generation: GcGenerationNumber },
    UnrootImmediateChildren,
    RootImmediateChildren,
}

#[derive(Clone, Copy, Debug)]
pub struct Tracer {
    pub(crate) operation: TraceOperation,
    pub(crate) id: GcTracerId,
}

pub trait Trace {
    fn trace(&self, tracer: &Tracer);

    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }
}

impl Tracer {
    pub(crate) fn new(operation: TraceOperation) -> Self {
        UUID_SOURCE.with(|source| {
            let uuid = source.get();
            source.set(uuid + 1);
            Self {
                operation,
                id: uuid,
            }
        })
    }

    pub fn trace<T: Trace>(&self, obj: &T) {
        if T::needs_trace() {
            obj.trace(self)
        }
    }
}
