use std::fmt::Debug;

use super::*;

pub struct Gc<T: Trace + 'static> {
    root_cell: RootGuardCell<T>,
    ptr: GcObjectPtr<T>,
}

impl<T: Trace + 'static> Gc<T> {
    pub fn new(data: T) -> Self {
        let ptr = GcHeap::alloc(data);

        assert!(
            unsafe { ptr.as_ref() }.is_dirty(),
            "allocated objects must be dirty"
        );

        Self {
            root_cell: RootGuardCell::new_rooted(ptr),
            ptr,
        }
    }

    pub fn as_ptr(&self) -> GcObjectPtr<T> {
        self.ptr
    }

    pub fn root(&self) -> bool {
        self.root_cell.root(self.ptr)
    }

    /// unrooting an accessible Gc<T> manually may deallocate the object it's pointing to
    /// prematurely, thus causing use-after-free
    pub unsafe fn unroot(&self) -> bool {
        self.root_cell.unroot()
    }

    pub fn is_rooted(&self) -> bool {
        self.root_cell.is_rooted()
    }

    pub fn with<F: FnOnce(&T) -> R, R>(&self, f: F) -> R {
        let borrow = self.borrow();
        f(&borrow)
    }

    pub fn with_mut<F: FnOnce(&mut T) -> R, R>(&self, f: F) -> R {
        let mut borrow_mut = self.borrow_mut();
        f(&mut borrow_mut)
    }

    /// Clones the gc pointer directly. Creates an unrooted Gc<T> if self is unrooted already and those are unsafe
    pub unsafe fn clone_unsafe(&self) -> Self {
        Self {
            root_cell: self.root_cell.clone(),
            ptr: self.ptr,
        }
    }
}

impl<T: Trace + 'static> Clone for Gc<T> {
    fn clone(&self) -> Self {
        Self {
            root_cell: RootGuardCell::new_rooted(self.ptr),
            ptr: self.ptr,
        }
    }
}

impl<T: Trace + 'static> Drop for Gc<T> {
    fn drop(&mut self) {
        self.root_cell.unroot();
    }
}

impl<T: Trace + 'static> Debug for Gc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rooted = if self.root_cell.is_rooted() {
            "rooted"
        } else {
            "unrooted"
        };

        f.debug_struct("Gc")
            .field("rooted", &rooted)
            .field("boxed", &self.ptr)
            .finish()
    }
}
