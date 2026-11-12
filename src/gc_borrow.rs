use std::{
    cell::{BorrowError, BorrowMutError, Ref, RefMut},
    ops::{Deref, DerefMut},
};

use super::*;

/// a borrow of a [`Gc<T>`]. Keeps the data and it's fields rooted while it's in use.
pub struct GcBorrow<'a, T: Trace + 'static> {
    borrow: Ref<'a, T>,
    _root_guard: RootGuard<T>,
}

/// a mutable borrow of a [`Gc<T>`]. Keeps the data and it's fields rooted while it's in use.
pub struct GcBorrowMut<'a, T: Trace + 'static> {
    borrow: RefMut<'a, T>,
    _root_guard: RootGuard<T>,
}

impl<T: Trace + 'static> Gc<T> {
    pub fn borrow<'a>(&self) -> GcBorrow<'a, T> {
        let boxed = unsafe { self.ptr.as_ref() };

        // rooting the object itself keeps it alive
        let _root_guard = RootGuard::new(self.ptr);

        // rooting it's fields means that any value moved out of the struct will be rooted
        Tracer::root_shallow().visit_from_ptr(self.ptr);

        GcBorrow {
            borrow: boxed.data.borrow(),
            _root_guard,
        }
    }

    pub fn try_borrow<'a>(&self) -> Result<GcBorrow<'a, T>, BorrowError> {
        let boxed = unsafe { self.ptr.as_ref() };
        let _ = boxed.data.try_borrow()?;

        // rooting the object itself keeps it alive
        let _root_guard = RootGuard::new(self.ptr);

        // rooting it's fields means that any value moved out of the struct will be rooted
        Tracer::root_shallow().visit_from_ptr(self.ptr);

        Ok(GcBorrow {
            borrow: boxed.data.try_borrow()?,
            _root_guard,
        })
    }

    pub unsafe fn borrow_unsafe<'a>(&self) -> GcBorrow<'a, T> {
        let boxed = unsafe { self.ptr.as_ref() };

        // rooting the object itself keeps it alive
        let _root_guard = RootGuard::new(self.ptr);

        GcBorrow {
            borrow: boxed.data.borrow(),
            _root_guard,
        }
    }

    pub unsafe fn try_borrow_unsafe<'a>(&self) -> Result<GcBorrow<'a, T>, BorrowError> {
        let boxed = unsafe { self.ptr.as_ref() };
        let _ = boxed.data.try_borrow()?;

        // rooting the object itself keeps it alive
        let _root_guard = RootGuard::new(self.ptr);

        Ok(GcBorrow {
            borrow: boxed.data.try_borrow()?,
            _root_guard,
        })
    }

    pub fn borrow_mut<'a>(&self) -> GcBorrowMut<'a, T> {
        let boxed = unsafe { self.ptr.as_ref() };

        // rooting the object itself keeps it alive
        let _root_guard = RootGuard::new(self.ptr);

        // rooting it's fields means that any value moved out of the struct will be rooted
        Tracer::root_shallow().visit_from_ptr(self.ptr);

        GcBorrowMut {
            borrow: boxed.data.borrow_mut(),
            _root_guard,
        }
    }

    pub fn try_borrow_mut<'a>(&self) -> Result<GcBorrowMut<'a, T>, BorrowMutError> {
        let boxed = unsafe { self.ptr.as_ref() };
        let _ = boxed.data.try_borrow_mut()?;

        // rooting the object itself keeps it alive
        let _root_guard = RootGuard::new(self.ptr);

        // rooting it's fields means that any value moved out of the struct will be rooted
        Tracer::root_shallow().visit_from_ptr(self.ptr);

        Ok(GcBorrowMut {
            borrow: boxed.data.try_borrow_mut()?,
            _root_guard,
        })
    }
}

impl<T: Trace + 'static> Deref for GcBorrow<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.borrow
    }
}

impl<T: Trace + 'static> Deref for GcBorrowMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.borrow
    }
}

impl<T: Trace + 'static> DerefMut for GcBorrowMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.borrow
    }
}
