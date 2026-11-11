use std::{cell::Cell, ptr::NonNull};

use super::*;

pub struct Gc<T: Trace + 'static> {
    pub(crate) rooted: Cell<bool>,
    pub(crate) boxed: NonNull<GcBox<T>>,
}

impl<T: Trace + 'static> Gc<T> {
    pub fn new(data: T) -> Self {
        Self {
            rooted: Cell::from(true),
            boxed: GcHeap::alloc_rooted(data),
        }
    }

    pub fn with<F: FnOnce(&T) -> R, R>(&self, f: F) -> R {
        let borrow = self.borrow();
        f(&borrow)
    }

    pub fn with_mut<F: FnOnce(&mut T) -> R, R>(&self, f: F) -> R {
        let mut borrow_mut = self.borrow_mut();
        f(&mut borrow_mut)
    }

    pub fn traced(&self, tracer: &Tracer) -> bool {
        unsafe { self.boxed.as_ref().last_tracer_id.get() == tracer.id }
    }
}

impl<T: Trace + 'static> Clone for Gc<T> {
    fn clone(&self) -> Self {
        unsafe {
            self.boxed.as_ref().inc_roots();
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
            unsafe { self.boxed.as_ref() }.dec_roots();
        }
    }
}
