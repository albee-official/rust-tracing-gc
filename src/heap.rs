use std::{
    cell::{Cell, RefCell},
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
    allocated: Vec<NonNull<dyn GcAlloc>>,
}

impl GcHeap {
    pub(crate) fn new() -> Self {
        Self {
            generation: 0,
            allocated: vec![],
        }
    }

    #[allow(unused)]
    pub fn live_count() -> usize {
        GLOBAL_HEAP.with_borrow(|h| h.allocated.len())
    }

    #[allow(unused)]
    pub(crate) fn alloc<T: Trace + 'static>(data: T) -> NonNull<GcBox<T>> {
        GLOBAL_HEAP.with_borrow_mut(|heap| {
            let boxed = Box::new(GcBox::new(data, heap));
            let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };
            heap.allocated.push(ptr);
            ptr
        })
    }

    pub(crate) fn alloc_rooted<T: Trace + 'static>(data: T) -> NonNull<GcBox<T>> {
        GLOBAL_HEAP.with_borrow_mut(|heap| {
            let boxed = Box::new(GcBox::new_rooted(data, heap));
            let ptr = unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) };
            heap.allocated.push(ptr);
            ptr
        })
    }

    pub fn collect() {
        GLOBAL_HEAP.with_borrow_mut(|heap| {
            heap.generation += 1;

            // unroot handles reachable by tracer (this makes sure that all Gc objects inside the heap are NOT roots)
            let unroot_tracer = Tracer::new(TraceOperation::UnrootImmediateChildren);

            for allocation in &heap.allocated {
                let alloc = unsafe { allocation.as_ref() };

                alloc.trace(&unroot_tracer);
            }

            let mark_tracer = Tracer::new(TraceOperation::Mark {
                generation: heap.generation,
            });

            for allocation in &heap.allocated {
                let alloc = unsafe { allocation.as_ref() };

                if alloc.is_rooted() {
                    alloc.trace(&mark_tracer);
                }
            }

            // sweep
            let current_generation = heap.generation;
            heap.allocated.retain_mut(|allocation| {
                let alive = unsafe { allocation.as_ref() }.is_alive(current_generation);
                if !alive {
                    drop(unsafe { Box::from_raw(allocation.as_ptr()) });
                }
                alive
            });
        });
    }
}

trait GcAlloc {
    fn is_alive(&self, current_generation: GcGenerationNumber) -> bool;
    fn is_rooted(&self) -> bool;
    fn trace(&self, tracer: &Tracer);
}

pub(crate) struct GcBox<T: Trace + 'static> {
    pub(crate) generation: Cell<GcGenerationNumber>,
    pub(crate) last_tracer_id: Cell<GcTracerId>, // allows for multiple recursive trace passes regardless of the generation
    pub(crate) roots: Cell<GcRootsNumber>,
    pub(crate) data: RefCell<T>,
}

impl<T: Trace + 'static> GcAlloc for GcBox<T> {
    fn is_alive(&self, current_generation: GcGenerationNumber) -> bool {
        self.is_rooted() || self.generation.get() == current_generation
    }

    fn is_rooted(&self) -> bool {
        self.roots.get() > 0
    }

    fn trace(&self, tracer: &Tracer) {
        if let Ok(data) = self.data.try_borrow() {
            data.trace(tracer);
        }
    }
}

impl<T: Trace + 'static> GcBox<T> {
    pub fn new(data: T, heap: &GcHeap) -> Self {
        Self {
            generation: Cell::new(heap.generation),
            last_tracer_id: Cell::new(0),
            roots: Cell::new(0),
            data: RefCell::from(data),
        }
    }

    pub fn new_rooted(data: T, heap: &GcHeap) -> Self {
        Self {
            generation: Cell::new(heap.generation),
            last_tracer_id: Cell::new(0),
            roots: Cell::new(1),
            data: RefCell::from(data),
        }
    }

    pub fn inc_roots(&self) {
        self.roots.update(|roots| roots + 1);
    }

    pub fn dec_roots(&self) {
        self.roots.update(|roots| roots - 1);
    }
}

impl Drop for GcHeap {
    fn drop(&mut self) {
        for ptr in self.allocated.drain(..) {
            drop(unsafe { Box::from_raw(ptr.as_ptr()) });
        }
    }
}
