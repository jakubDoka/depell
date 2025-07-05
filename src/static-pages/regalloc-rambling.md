When we allocate registers and want to reduce the size of the interference
graph, we can assume values that must be in a specific register for program to
even function, we can remove the register for the possible registers int the
neighbors and drop the edges.

This process also uncovers cases where we need to split a live-range, that is if possible registers of use do not overlap with possible registers of a def. We need to fail the live-range, insert a split and retry the allocation. How do we split tho? Let's go over some scenarios.

### 1. One use interferes but majority are fine

```rust
fn foo(a: usize) -> usize {
    let b = a + a;
    return bar(a, b);
}
```

```ir
fn foo(a) {
    v1 = a + a
    ret bar(a, v1)
}
```

First pass:

```asm
foo:
    l1:
        def = a
    a  := arg   = l1
    v1 := + a a = l1
    ret bar(a, v1) 
    // l1: impossible register
    // - must split
    // - best split is before modifying use in v1
    // - if we iterate uses and also descend trough those with the same live
    // range and count the uses, same for defs.
    // - prefer to split on single use and single def sites, in this case we
    // have 3 choices: a-v1[0] a-v1[1] a-bar v1-bar
```

