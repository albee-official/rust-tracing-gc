use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    ptr::NonNull,
};

use super::*;

pub type GcGenerationNumber = u64;
pub type GcRootsNumber = u16;

thread_local! {
    static GLOBAL_HEAP: RefCell<GcHeap> = RefCell::new(GcHeap::new());
}

pub struct GcHeap {
    generation: GcGenerationNumber,
    objects: Vec<NonNull<dyn GcAlloc>>,
}

impl GcHeap {
    pub(crate) fn new() -> Self {
        Self {
            generation: 0,
            objects: vec![],
        }
    }

    #[allow(unused)]
    pub fn live_count() -> usize {
        GLOBAL_HEAP.with_borrow(|h| h.objects.len())
    }

    pub fn alloc<T: Trace + 'static>(data: T) -> NonNull<GcObject<T>> {
        GLOBAL_HEAP.with_borrow_mut(|heap| {
            let boxed = Box::new(GcObject::new(data, heap));
            let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };
            heap.objects.push(ptr);
            ptr
        })
    }

    pub fn collect() {
        GLOBAL_HEAP.with_borrow_mut(|heap| {
            heap.generation += 1;

            // unroot handles reachable by tracer (this makes sure that all Gc objects inside the heap are NOT roots)
            let mut unroot_tracer = Tracer::unroot_shallow();

            for allocation in &heap.objects {
                let alloc = unsafe { allocation.as_ref() };

                alloc.trace_data(&mut unroot_tracer);
                unroot_tracer.visit_stack();
            }

            let mut mark_tracer = Tracer::mark_deep(heap.generation);

            for allocation in &heap.objects {
                let alloc = unsafe { allocation.as_ref() };

                if alloc.is_rooted() {
                    alloc.trace_data(&mut mark_tracer);
                    mark_tracer.visit_stack();
                }
            }

            // sweep
            let current_generation = heap.generation;
            heap.objects.retain_mut(|allocation| {
                let alive = unsafe { allocation.as_ref() }.is_alive(current_generation);
                if !alive {
                    drop(unsafe { Box::from_raw(allocation.as_ptr()) });
                }
                alive
            });
        });
    }
}

pub trait GcAlloc {
    fn is_alive(&self, current_generation: GcGenerationNumber) -> bool;
    fn is_rooted(&self) -> bool;
    fn trace_data(&self, tracer: &mut Tracer);
}

pub struct GcObject<T: Trace + 'static> {
    pub(crate) generation: Cell<GcGenerationNumber>,
    pub(crate) last_deep_tracer_id: Cell<GcTracerId>, // allows for multiple recursive trace passes regardless of the generation
    pub(crate) roots: Cell<GcRootsNumber>,
    pub(crate) data: RefCell<T>,
}

impl<T: Trace + 'static> GcAlloc for GcObject<T> {
    fn is_alive(&self, current_generation: GcGenerationNumber) -> bool {
        self.is_rooted() || self.generation.get() == current_generation
    }

    fn is_rooted(&self) -> bool {
        self.roots() > 0
    }

    fn trace_data(&self, tracer: &mut Tracer) {
        if let Ok(data) = self.data.try_borrow() {
            tracer.trace(data.deref());
        } else {
            // if data is mutably borrowed,
            // then all of it's members are rooted,
            // thus they will be visited by the mark anyway so it's safe to just skip it
        }
    }
}

impl<T: Trace + 'static> GcObject<T> {
    pub fn new(data: T, heap: &GcHeap) -> Self {
        Self {
            generation: Cell::new(heap.generation),
            last_deep_tracer_id: Cell::new(0),
            roots: Cell::new(0),
            data: RefCell::from(data),
        }
    }

    fn roots(&self) -> GcRootsNumber {
        self.roots.get()
    }

    fn inc_roots(&self) {
        self.roots.update(|roots| roots + 1);
    }

    fn dec_roots(&self) {
        self.roots.update(|roots| roots - 1);
    }
}

impl Drop for GcHeap {
    fn drop(&mut self) {
        for ptr in self.objects.drain(..) {
            drop(unsafe { Box::from_raw(ptr.as_ptr()) });
        }
    }
}

pub(crate) struct RootGuard<T: Trace + 'static> {
    ptr: NonNull<GcObject<T>>,
}

impl<T: Trace + 'static> RootGuard<T> {
    pub fn new(ptr: NonNull<GcObject<T>>) -> Self {
        let boxed = unsafe { ptr.as_ref() };
        boxed.inc_roots();

        Self { ptr }
    }

    #[allow(unused)]
    pub fn get_ptr(&self) -> NonNull<GcObject<T>> {
        self.ptr
    }
}

impl<T: Trace + 'static> Drop for RootGuard<T> {
    fn drop(&mut self) {
        let boxed = unsafe { self.ptr.as_ref() };
        boxed.dec_roots();
    }
}

impl<T: Trace + 'static> Clone for RootGuard<T> {
    fn clone(&self) -> Self {
        Self::new(self.ptr)
    }
}

/// Cell<Option<RootGuard<T>>> wraper
pub struct RootGuardCell<T: Trace + 'static> {
    pub(crate) guard: Cell<Option<RootGuard<T>>>,
}

impl<T: Trace + 'static> RootGuardCell<T> {
    pub fn new_rooted(boxed: NonNull<GcObject<T>>) -> Self {
        Self {
            guard: Cell::new(Some(RootGuard::new(boxed))),
        }
    }

    pub fn new_unrooted() -> Self {
        Self {
            guard: Cell::new(None),
        }
    }

    pub fn is_rooted(&self) -> bool {
        let value = unsafe { self.guard.as_ptr().as_ref_unchecked() };
        match value {
            Some(_) => false,
            None => true,
        }
    }

    pub fn get_ptr(&self) -> Option<NonNull<GcObject<T>>> {
        let value = unsafe { self.guard.as_ptr().as_ref_unchecked() };
        match value {
            Some(guard) => Some(guard.ptr),
            None => None,
        }
    }

    /// true if guard updated it's pointer
    pub fn root(&self, ptr: NonNull<GcObject<T>>) -> bool {
        let old = self.guard.take();
        if let Some(g) = &old {
            if g.ptr == ptr {
                self.guard.set(old);
                return false;
            }
        }
        self.guard.set(Some(RootGuard::new(ptr)));
        true
    }

    /// true if something was rooted
    pub fn unroot(&self) -> bool {
        match self.guard.take() {
            Some(_) => {
                self.guard.set(None);
                true
            }

            None => false,
        }
    }
}

impl<T: Trace> Clone for RootGuardCell<T> {
    fn clone(&self) -> Self {
        match self.get_ptr() {
            Some(ptr) => Self::new_rooted(ptr),
            None => Self::new_unrooted(),
        }
    }
}
