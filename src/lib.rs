mod gc;
mod heap;
mod tracer;

pub use gc::*;
pub use heap::*;
pub use tracer::*;

pub(crate) struct RootGuard<'a, T: Trace + 'static> {
    boxed: &'a GcBox<T>,
}

impl<'a, T: Trace> RootGuard<'a, T> {
    pub fn new(boxed: &'a GcBox<T>) -> Self {
        boxed.root();
        Self { boxed }
    }
}

impl<T: Trace> Drop for RootGuard<'_, T> {
    fn drop(&mut self) {
        self.boxed.unroot();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    struct Node {
        next: Option<Gc<Node>>,
        drops: Rc<Cell<usize>>,
    }

    impl Trace for Node {
        fn trace(&self, t: &Tracer) {
            if let Some(n) = &self.next {
                t.trace(n);
            }
        }
    }

    impl Drop for Node {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }

    fn node(drops: &Rc<Cell<usize>>) -> Gc<Node> {
        Gc::new(Node {
            next: None,
            drops: drops.clone(),
        })
    }
    #[test]
    fn unreferenced_is_freed() {
        let d = Rc::new(Cell::new(0));
        drop(node(&d));
        GcHeap::collect();

        assert_eq!(d.get(), 1);
        assert_eq!(GcHeap::live_count(), 0);
    }

    #[test]
    fn rooted_survives() {
        let d = Rc::new(Cell::new(0));
        let a = node(&d);
        GcHeap::collect();
        GcHeap::collect(); // twice: catches roots underflow

        assert_eq!(d.get(), 0);
        a.with(|n| assert!(n.next.is_none()));
    }

    #[test]
    fn garbage_cycle_is_freed() {
        let d = Rc::new(Cell::new(0));
        let a = node(&d);
        let b = node(&d);
        a.with_mut(|n| n.next = Some(b.clone()));
        b.with_mut(|n| n.next = Some(a.clone()));
        drop((a, b));
        GcHeap::collect();
        assert_eq!(d.get(), 2);
        assert_eq!(GcHeap::live_count(), 0);
    }

    #[test]
    fn cycle_kept_by_one_outside_handle() {
        let d = Rc::new(Cell::new(0));
        let a = node(&d);
        let b = node(&d);
        a.with_mut(|n| n.next = Some(b.clone()));
        b.with_mut(|n| n.next = Some(a.clone()));
        drop(b);
        GcHeap::collect();
        assert_eq!(d.get(), 0);
        a.with(|n| assert!(n.next.is_some()));
    }

    #[test]
    fn handle_taken_out_stays_alive() {
        let d = Rc::new(Cell::new(0));
        let a = node(&d);
        a.with_mut(|n| n.next = Some(node(&d)));
        GcHeap::collect(); // child is now unrooted, inside a
        let child = a.with_mut(|n| n.next.take()).unwrap();
        GcHeap::collect(); // must not free child
        assert_eq!(d.get(), 0);
        child.with(|_| ());
    }

    #[test]
    fn collect_inside_with_mut() {
        let d = Rc::new(Cell::new(0));
        let a = node(&d);
        a.with_mut(|n| n.next = Some(node(&d)));
        GcHeap::collect();
        a.with_mut(|n| {
            GcHeap::collect(); // a is borrowed mutably here
            assert!(n.next.is_some());
        });
        assert_eq!(d.get(), 0);
    }

    #[test]
    fn stress() {
        let d = Rc::new(Cell::new(0));
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut rng = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        let iters = if cfg!(miri) { 200 } else { 20_000 };
        let mut handles: Vec<Gc<Node>> = vec![];

        for _ in 0..iters {
            match rng() % 6 {
                0 => handles.push(node(&d)),
                1 if !handles.is_empty() => {
                    let i = rng() as usize % handles.len();
                    let j = rng() as usize % handles.len();
                    let target = handles[j].clone();
                    handles[i].with_mut(|n| n.next = Some(target)); // may build cycles
                }
                2 if !handles.is_empty() => {
                    let i = rng() as usize % handles.len();
                    handles.swap_remove(i); // drop a handle
                }
                3 if !handles.is_empty() => {
                    let i = rng() as usize % handles.len();
                    if let Some(h) = handles[i].with_mut(|n| n.next.take()) {
                        handles.push(h);
                    }
                }
                4 => GcHeap::collect(),
                5 if !handles.is_empty() => {
                    let i = rng() as usize % handles.len();
                    handles[i].with_mut(|_| GcHeap::collect());
                }
                _ => {}
            }
        }

        drop(handles);
        GcHeap::collect();
        assert_eq!(GcHeap::live_count(), 0); // everything must eventually be freed
    }
}
