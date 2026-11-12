use std::{cell::Cell, ptr::NonNull};

use super::*;

pub(crate) type GcTracerId = u64;

thread_local! {
    static ID_SOURCE: Cell<GcTracerId> = Cell::from(1);
}

#[derive(Clone, Copy, Debug)]
pub enum TraceOperation {
    Mark { generation: GcGenerationNumber },
    Unroot,
    Root,
}

#[derive(Clone, Copy, Debug)]
pub enum TraceTraversal {
    Deep { id: GcTracerId },
    Shallow,
}

#[derive(Clone)]
pub struct Tracer {
    pub(crate) operation: TraceOperation,
    pub(crate) traversal: TraceTraversal,
    pub(crate) visit_queue: Vec<NonNull<dyn GcAlloc>>,
}

pub trait Trace {
    fn trace(&self, tracer: &mut Tracer);

    fn needs_trace() -> bool
    where
        Self: Sized,
    {
        true
    }
}

impl Tracer {
    pub fn new_tracer_id() -> GcTracerId {
        ID_SOURCE.with(|source| {
            let id = source.get();
            source.set(id + 1);
            id
        })
    }

    pub fn mark_deep(generation: GcGenerationNumber) -> Self {
        Self::new(
            TraceOperation::Mark { generation },
            TraceTraversal::Deep {
                id: Self::new_tracer_id(),
            },
        )
    }

    pub fn root_shallow() -> Self {
        Self::new(TraceOperation::Root, TraceTraversal::Shallow)
    }

    pub fn unroot_shallow() -> Self {
        Self::new(TraceOperation::Unroot, TraceTraversal::Shallow)
    }

    fn new(operation: TraceOperation, traversal: TraceTraversal) -> Self {
        ID_SOURCE.with(|source| {
            let id = source.get();
            source.set(id + 1);
            Self {
                operation,
                traversal,
                visit_queue: vec![],
            }
        })
    }

    pub fn was_last_visitor_of<T: Trace>(&self, ptr: NonNull<GcObject<T>>) -> bool {
        match self.traversal {
            TraceTraversal::Deep { id } => unsafe { ptr.as_ref() }.last_deep_tracer_id.get() == id,
            TraceTraversal::Shallow => false,
        }
    }

    /// visits all objects reachable from obj
    pub fn visit_from<T: Trace>(&mut self, obj: &Gc<T>) {
        self.queue_next(obj);
        self.visit_queue();
    }

    /// visits all objects reachable from ptr to the obj
    pub(crate) fn visit_from_ptr<T: Trace>(&mut self, ptr: NonNull<GcObject<T>>) {
        self.queue_next_ptr(ptr);
        self.visit_queue();
    }

    /// visits all queued objects
    pub fn visit_queue(&mut self) {
        while let Some(next) = self.visit_queue.pop() {
            let boxed = unsafe { next.as_ref() };
            boxed.trace_data(self);
        }
    }

    pub fn trace<T: Trace>(&mut self, obj: &T) {
        if T::needs_trace() {
            obj.trace(self);
        }
    }

    /// queues object to be visited without actually visiting it
    pub fn queue_next<T: Trace>(&mut self, obj: &Gc<T>) {
        self.visit_queue.push(obj.ptr);
    }

    /// queues object to be visited without actually visiting it
    pub(crate) fn queue_next_ptr<T: Trace>(&mut self, ptr: NonNull<GcObject<T>>) {
        // println!(
        //     "queue next: {:?}, {}",
        //     std::ptr::from_ref(boxed),
        //     self.visit_queue.len()
        // );

        self.visit_queue.push(ptr);
    }
}
