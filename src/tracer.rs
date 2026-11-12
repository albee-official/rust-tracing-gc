use std::cell::{Cell, RefCell};

use super::*;

pub(crate) type GcTracerId = u64;
pub(crate) type VisitStack = Vec<GcErasedObjectPtr>;
pub type TracerFnVisitor = Box<dyn FnMut(GcErasedObjectPtr)>;

thread_local! {
    static ID_SOURCE: Cell<GcTracerId> = Cell::from(1);
    static VISIT_STACK_POOL: RefCell<Vec<VisitStack>> = RefCell::new(vec![]);
}

pub enum TraceOperation {
    Mark { generation: GcGenerationNumber },
    Unroot,
    Root,
    VisitFn { visitor: TracerFnVisitor },
}

pub enum TraceTraversal {
    Deep { id: GcTracerId },
    Shallow,
}

pub struct Tracer {
    pub(crate) operation: TraceOperation,
    pub(crate) traversal: TraceTraversal,
    pub(crate) visit_stack: VisitStack,
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
    fn new_tracer_id() -> GcTracerId {
        ID_SOURCE.with(|source| {
            let id = source.get();
            source.set(id + 1);
            id
        })
    }

    fn get_visit_stack() -> VisitStack {
        VISIT_STACK_POOL.with_borrow_mut(|pool| pool.pop().unwrap_or_else(VisitStack::new))
    }

    fn return_visit_stack(mut queue: VisitStack) {
        queue.clear();
        VISIT_STACK_POOL.with_borrow_mut(|pool| {
            if pool.len() > 4 {
                std::mem::drop(queue);
            } else {
                pool.push(queue);
            }
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

    pub unsafe fn unroot_shallow() -> Self {
        Self::new(TraceOperation::Unroot, TraceTraversal::Shallow)
    }

    pub fn visit_shallow(visitor: TracerFnVisitor) -> Self {
        Self::new(TraceOperation::VisitFn { visitor }, TraceTraversal::Shallow)
    }

    fn new(operation: TraceOperation, traversal: TraceTraversal) -> Self {
        Self {
            operation,
            traversal,
            visit_stack: Self::get_visit_stack(),
        }
    }

    pub fn was_last_visitor_of<T: Trace>(&self, ptr: GcObjectPtr<T>) -> bool {
        match self.traversal {
            TraceTraversal::Deep { id } => unsafe { ptr.as_ref() }.get_last_deep_tracer_id() == id,
            TraceTraversal::Shallow => false,
        }
    }

    /// visits all objects reachable from obj
    pub fn visit_from<T: Trace>(&mut self, obj: &Gc<T>) {
        self.visit_from_ptr(obj.as_ptr())
    }

    /// visits all objects reachable from ptr to the obj
    pub(crate) fn visit_from_ptr<T: Trace>(&mut self, ptr: GcObjectPtr<T>) {
        self.push_next_ptr(ptr);
        self.visit_stack();
    }

    /// visits all objects inside the stack
    pub fn visit_stack(&mut self) {
        while let Some(next) = self.visit_stack.pop() {
            let boxed = unsafe { next.as_ref() };
            boxed.trace_data(self);
        }
    }

    pub fn trace<T: Trace>(&mut self, obj: &T) {
        if T::needs_trace() {
            obj.trace(self);
        }
    }

    /// pushes object ponted by the Gc ptr to the visit stack
    pub fn push_next<T: Trace>(&mut self, obj: &Gc<T>) {
        self.push_next_ptr(obj.as_ptr())
    }

    /// pushes object ponted by the raw ptr to the visit stack
    pub fn push_next_ptr<T: Trace>(&mut self, ptr: GcObjectPtr<T>) {
        self.visit_stack.push(ptr);
    }
}

impl Drop for Tracer {
    fn drop(&mut self) {
        let queue = std::mem::take(&mut self.visit_stack);
        Self::return_visit_stack(queue);
    }
}
