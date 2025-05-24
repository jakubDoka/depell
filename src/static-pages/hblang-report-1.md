# Hblang Progress Report 1

It's bin quite some time since I made the first
[blog](/blogs/developing-hblang). Since then I managed to implement some of the
things I mentioned under the category of static analysis done by the optimizer.
Lets demonstrate, starting with basic case of returning stack memory from a
function:

```hb
main := fn(): ^uint {
    return &0
}
```

We can now also detect loop invariant breaks out of the loop:

```hb
main := fn(): void {
    i := 0
    loop if i == 10 break else {
        // i += 1 // maybe this was the intent?
    }
}
```

And finally, we can detect some trivial index out of bounds mistakes (when it
comes to constant offsets):

```hb
main := fn(): int {
    return int.[0, 1, 2][3]
}
```

Its not that impressive yet but it can catch common mistakes people make
regularly.

Besides these features, the language frontend is now lot smarter when
evaluating comptime code, this it to the point we can do stuff like this:

```hb
main := fn(): uint {
    val: Array(uint, 3) = .(1, .(2, .(3, .())))
    return val.get(0) + val.get(1) + val.get(2)
}

Array := fn(E: type, len: uint): type if len == 0 {
    return struct {
        get := fn(self: @CurrentScope(), i: uint): E die
    }
} else {
    // not quite general enough to do this inline yet
    Next := Array(E, len - 1)
    return struct {
        .elem: E;
        .next: Next

        get := fn(self: @CurrentScope(), i: uint): E {
            if i == 0 return self.elem

            return self.next.get(i - 1)
        }
    }
}
```

The array function is actually compiled into holybytes bytecode and called
recursively during compile time to build the array struct. There are still
things that need to be supported like capturing arbitrary values inside struct
scopes, for now only `type` variables are supported.

### Zig rewrite

Less important topic worth mentioning is that the compiler and also the build
process of depell was rewritten in zig. The reason is simple, the memory model
of rust is just not that good of a fit for Sea of Nodes way of doing things. I
just want to use pointers when it comes to the graph representation. Whereas in
rust you need to resort to array of nodes and indices and that brings a level
of indirection to every graph manipulation one does.

### Next Steps

- x86_64 backend
- more optimizations
- more static analysis
- more fine grained layout specification for types
- more complete partial evaluation
