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
    const NEEDS_TRACE: bool = true;
    fn trace(&self, tracer: &Tracer);
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

    pub fn trace<T: Trace>(&self, obj: &Gc<T>) {
        match self.operation {
            TraceOperation::UnrootImmediateChildren | TraceOperation::RootImmediateChildren => {
                obj.trace(self)
            }

            TraceOperation::Mark { .. } => {
                if !obj.traced(self) {
                    obj.trace(self)
                }
            }
        }
    }
}
