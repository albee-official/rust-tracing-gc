use std::cell::Cell;

use super::*;

thread_local! {
    static UUID_SOURCE: Cell<u32> = Cell::from(1);
}

#[derive(Clone, Copy, Debug)]
pub enum TraceOperation {
    Mark { generation: u32 },
    UnrootImmediateChildren,
    RootImmediateChildren,
}

#[derive(Clone, Copy, Debug)]
pub struct Tracer {
    pub(crate) operation: TraceOperation,
    pub(crate) uuid: u32,
}

pub trait Trace {
    fn trace(&self, tracer: &Tracer);
}

impl Tracer {
    pub(crate) fn new(operation: TraceOperation) -> Self {
        UUID_SOURCE.with(|source| {
            let uuid = source.get();
            source.set(uuid + 1);
            Self { operation, uuid }
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
