use std::{cell::Cell, ptr::NonNull};

use super::*;

pub struct Gc<T: Trace + 'static> {
    rooted: Cell<bool>,
    boxed: NonNull<GcBox<T>>,
}

impl<T: Trace + 'static> Gc<T> {
    pub fn new(data: T) -> Self {
        Self {
            rooted: Cell::from(true),
            boxed: GcHeap::alloc_rooted(data),
        }
    }

    pub fn with<F: FnOnce(&T) -> R, R>(&self, f: F) -> R {
        let boxed = unsafe { self.boxed.as_ref() };
        let _guard = RootGuard::new(boxed);
        boxed
            .data
            .borrow()
            .trace(&Tracer::new(TraceOperation::RootImmediateChildren));

        let data = boxed.data.borrow();
        let res = f(&data);
        res
    }

    pub fn with_mut<F: FnOnce(&mut T) -> R, R>(&self, f: F) -> R {
        let boxed = unsafe { self.boxed.as_ref() };
        let _guard = RootGuard::new(boxed);
        boxed
            .data
            .borrow()
            .trace(&Tracer::new(TraceOperation::RootImmediateChildren));

        let mut data = boxed.data.borrow_mut();
        let res = f(&mut data);
        res
    }

    pub fn traced(&self, tracer: &Tracer) -> bool {
        unsafe { self.boxed.as_ref().last_tracer_uuid.get() == tracer.uuid }
    }

    pub fn trace(&self, tracer: &Tracer) {
        let boxed = unsafe { self.boxed.as_ref() };

        match tracer.operation {
            TraceOperation::Mark { generation } => {
                boxed.last_tracer_uuid.set(tracer.uuid);
                boxed.generation.set(generation);

                if let Ok(data) = boxed.data.try_borrow() {
                    data.trace(tracer);
                }
            }

            TraceOperation::RootImmediateChildren => {
                if !self.rooted.replace(true) {
                    boxed.root()
                }
            }

            TraceOperation::UnrootImmediateChildren => {
                if self.rooted.replace(false) {
                    boxed.unroot()
                }
            }
        }
    }
}

impl<T: Trace + 'static> Clone for Gc<T> {
    fn clone(&self) -> Self {
        unsafe {
            self.boxed.as_ref().root();
        }

        Self {
            rooted: Cell::from(true),
            boxed: self.boxed,
        }
    }
}

impl<T: Trace + 'static> Drop for Gc<T> {
    fn drop(&mut self) {
        if self.rooted.get() {
            unsafe { self.boxed.as_ref() }.unroot();
        }
    }
}
