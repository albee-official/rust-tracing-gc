use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    ptr::NonNull,
};

use super::*;

pub type GcGenerationNumber = u64;
pub type GcRootsNumber = usize;
pub type GcObjectPtr<T> = NonNull<GcObject<T>>;
pub type GcErasedObjectPtr = NonNull<dyn GcAlloc>;

thread_local! {
    static GLOBAL_HEAP: RefCell<GcHeap> = RefCell::new(GcHeap::new());
}

pub struct GcHeap {
    generation: GcGenerationNumber,
    dirty_objects: Vec<GcErasedObjectPtr>,
    objects: Vec<GcErasedObjectPtr>,
}

impl GcHeap {
    pub(crate) fn new() -> Self {
        Self {
            generation: 0,
            dirty_objects: vec![],
            objects: vec![],
        }
    }

    #[allow(unused)]
    pub fn live_count() -> usize {
        GLOBAL_HEAP.with_borrow(|h| h.objects.len())
    }

    pub fn alloc<T: Trace + 'static>(data: T) -> GcObjectPtr<T> {
        GLOBAL_HEAP.with_borrow_mut(|heap| {
            let boxed = Box::new(GcObject::new(data, heap));
            let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };
            heap.objects.push(ptr);
            heap.dirty_objects.push(ptr);
            ptr
        })
    }

    pub fn collect() {
        GLOBAL_HEAP.with_borrow_mut(|heap| {
            heap.generation += 1;

            // unroot handles reachable by tracer (this makes sure that all Gc objects inside the heap are NOT roots)
            let mut unroot_tracer = unsafe { Tracer::unroot_shallow() };

            for ptr in heap.dirty_objects.drain(..) {
                let obj = unsafe { ptr.as_ref() };
                assert!(obj.is_dirty(), "object should be dirty");

                obj.trace_data(&mut unroot_tracer);
                unroot_tracer.visit_stack();

                obj.clear_dirty();
            }

            let mut mark_tracer = Tracer::mark_deep(heap.generation);

            for ptr in &heap.objects {
                let obj = unsafe { ptr.as_ref() };

                if obj.is_rooted() {
                    obj.trace_data(&mut mark_tracer);
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

    pub fn objects<'a>(&'a self) -> &'a [GcErasedObjectPtr] {
        &self.objects
    }

    pub fn dirty_objects<'a>(&'a self) -> &'a [GcErasedObjectPtr] {
        &self.dirty_objects
    }

    pub fn with<F: FnOnce(&GcHeap) -> R, R>(f: F) -> R {
        GLOBAL_HEAP.with_borrow(f)
    }
}

pub trait GcAlloc {
    fn is_alive(&self, current_generation: GcGenerationNumber) -> bool;
    fn get_last_deep_tracer_id(&self) -> TracerId;
    fn set_last_deep_tracer_id(&self, id: TracerId);
    fn set_generation(&self, id: GcGenerationNumber);
    fn is_rooted(&self) -> bool;
    fn mark_dirty(&self) -> bool;
    fn clear_dirty(&self) -> bool;
    fn is_dirty(&self) -> bool;
    fn trace_data(&self, tracer: &mut Tracer);
}

#[derive(Debug)]
pub struct GcObject<T: Trace + 'static> {
    generation: Cell<GcGenerationNumber>,
    last_deep_tracer_id: Cell<TracerId>, // allows for multiple recursive trace passes regardless of the generation
    roots: Cell<GcRootsNumber>,
    dirty: Cell<bool>,

    pub(crate) data: RefCell<T>,
}

impl<T: Trace + 'static> GcAlloc for GcObject<T> {
    fn is_alive(&self, current_generation: GcGenerationNumber) -> bool {
        self.is_rooted() || self.generation.get() == current_generation
    }

    fn set_generation(&self, num: GcGenerationNumber) {
        self.generation.set(num);
    }

    fn get_last_deep_tracer_id(&self) -> TracerId {
        self.last_deep_tracer_id.get()
    }

    fn set_last_deep_tracer_id(&self, id: TracerId) {
        self.last_deep_tracer_id.set(id)
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

    fn mark_dirty(&self) -> bool {
        let was_clear = !self.dirty.replace(true);
        if was_clear {
            GLOBAL_HEAP.with_borrow_mut(|heap| {
                heap.dirty_objects.push(NonNull::from(self));
            })
        }

        was_clear
    }

    /// does not automatically remove from dirty_objects vec inside the heap
    fn clear_dirty(&self) -> bool {
        self.dirty.replace(false)
    }

    fn is_dirty(&self) -> bool {
        self.dirty.get()
    }
}

impl<T: Trace + 'static> GcObject<T> {
    pub(crate) fn new(data: T, heap: &mut GcHeap) -> Self {
        Self {
            generation: Cell::new(heap.generation),
            last_deep_tracer_id: Cell::new(0),
            dirty: Cell::from(true),
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
    ptr: GcObjectPtr<T>,
}

impl<T: Trace + 'static> RootGuard<T> {
    pub fn new(ptr: GcObjectPtr<T>) -> Self {
        let boxed = unsafe { ptr.as_ref() };
        boxed.inc_roots();

        Self { ptr }
    }

    #[allow(unused)]
    pub fn get_ptr(&self) -> GcObjectPtr<T> {
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
    pub fn new_rooted(boxed: GcObjectPtr<T>) -> Self {
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
            Some(_) => true,
            None => false,
        }
    }

    pub fn get_ptr(&self) -> Option<GcObjectPtr<T>> {
        let value = unsafe { self.guard.as_ptr().as_ref_unchecked() };
        match value {
            Some(guard) => Some(guard.ptr),
            None => None,
        }
    }

    /// true if guard updated it's pointer
    pub fn root(&self, ptr: GcObjectPtr<T>) -> bool {
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
