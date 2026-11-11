# rust-tracing-gc

A single-threaded mark-and-sweep garbage collector for Rust, with cycle collection.

Built as a learning project to explore how a tracing GC can work on top of Rust's ownership model. Not as a production library.

## Example

```rust
struct Node {
    next: Option<Gc<Node>>,
}

impl Trace for Node {
    fn trace(&self, t: &Tracer) {
        if let Some(n) = &self.next {
            t.trace(n);
        }
    }
}

let a = Gc::new(Node { next: None });
let b = Gc::new(Node { next: None });

// Build a cycle
a.with_mut(|n| n.next = Some(b.clone()));
b.with_mut(|n| n.next = Some(a.clone()));

drop((a, b));
GcHeap::collect(); // the cycle is unreachable, so both nodes are freed
```

## How it works

Every `Gc<T>` handle is either **rooted** (lives outside the heap, e.g. on the stack) or **unrooted** (stored inside a heap object). Each allocation keeps a count of its rooted handles. Handles carry their own `rooted` flag.

`GcHeap::collect()` runs three passes:

1. **Unroot:** visit every object in the heap, reachable or not, and clear the flag on each handle stored inside it, decrementing the target's root count. Afterwards, root counts reflect only handles outside the heap.
2. **Mark:** trace from objects with a nonzero root count, stamping each reached object with the current generation.
3. **Sweep:** free every object that is neither rooted nor marked.

Unrooting the whole heap, instead of only what the tracer reaches, is what lets garbage cycles be collected.

Access goes through `Gc::with` and `Gc::with_mut`. Before running the closure, they root the object and the handles inside it, so anything moved out of the heap stays protected, and a collection can safely run inside the closure.

## Safety model

The design relies on two rules the library cannot fully enforce:

- Every type's `Trace` impl must visit every `Gc` it contains.
- `Gc` handles must leave a heap object only through `with` / `with_mut`.

Breaking either can cause use-after-free. A derive macro for `Trace` is planned to remove the first risk.

## Limitations

- Single-threaded: `Gc` is `!Send` and `!Sync`, and the heap is thread-local.
- Not generational, not incremental: collection is stop-the-world.
- Marking is recursive, so very long object chains can overflow the stack.
- `Drop` implementations of managed types must not allocate or collect, and must not access other `Gc` values.
- No weak references or finalizer ordering guarantees.

## Testing

- Unit tests for the core cases: unreferenced objects, rooted objects, garbage cycles, cycles kept alive by one outside handle, handles moved out of the heap, and collection inside `with_mut`.
- A randomized stress test that builds cycles, drops and moves handles, and collects at random points, asserting that nothing leaks.
- The suite passes under [Miri](https://github.com/rust-lang/miri) with the default settings, `-Zmiri-tree-borrows`, and `-Zmiri-strict-provenance`.

```sh
cargo test
cargo +nightly miri test
```

Passing Miri shows that the tested executions had no undefined behavior. It does not prove the collector correct in general.

## Roadmap

- `#[derive(Trace)]`
- Explicit mark stack instead of recursion
- Benchmarks against `Rc` on cyclic structures