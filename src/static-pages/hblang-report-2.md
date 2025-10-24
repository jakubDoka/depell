# Hblang Progress Report 2

## New Register Allocator

During the implementation of the x86_64 backend I realized the current regalloc
was horrible. The implementation was simple, but it made the instruction
emission extremely tedious and buggy. The problem was that the old regalloc did
not emit split instructions, it just assigned ids to defs and if the ID was
exceeding the available register count, it would leave that to be handled by
the code emission.

Main problem with that approach is that you need temporary registers that can
not be used by anything else which makes the resulting code spill more then
necessary. But that's not even the worst part. Emitting the instructions
correctly was extremely tedious with endless amount of edge cases for every
instruction.

After I somehow made the x64 backend work, I decided to rework the regalloc
with the help of [Simple](https://github.com/SeaOfNodes/Simple) tutorial, I
managed to write a decent graph coloring regalloc algorithm that made the code
emission incredibly easy.

The key property of the new approach was that it worked with bitmasks
representing possible registers for each value in the ssa form. For example,
function arguments can only be in a single register so the def bitmaks would
reflect that. Another case is the return value input, that can only be one
register as well. Lets say you write a code like this:

```hb
main := fn(): uint {
    return foo(1)
}

foo := fn(a: uint): uint {
    return a
}
```

Depending on the call convention, the first argument could end up in a
different register from the return value. The new regalloc detects this and
inserts the split instruction, because `def_mask(a) & in_mask(ret, 0)` is
empty. This also solves the handling of spills. Instructions always expect the
input and output to be inside a register while split instructions allow spill
slots in their masks. This means that split of a split can reside on stack and
maps to a store and load respectively. Instruction emission stage no longer
needs to concern it self with inserting moves, it simply scans trough a basic
block and emits instructions in a very linear fashion.

Another improvement is handling of phi nodes and 2 address instructions. The
allocator now builds live-ranges that are unified on phi inputs and 2 address
inputs. If doing such unification results into impossible register allocation,
then splits are inserted. In most cases this eliminates lot of moves. In
contrast, the old approach was simply expecting the instruction emission to
insert the moves which resulted many more move instructions.

## New Backends

Hblang now supports compiling to wasm-freestanding and x86_64-linux targets.

### x86_64 Backend

The x64 backend was a great learning experience. At first I used the
[Zydis](https://github.com/zyantific/zydis) library to emit the instructions
because I thought it will simplify the implementation, but after I rewrote the
regalloc, this became so simple I was confident enough to remove the Zydis
assembler calls and just emit instructions manually. Once you get used to how
x86 instruction encodings work, implementing them picks up the pace and is not
even that hard.

The manual encoding payed off in the end. The instruction emission is orders of
magnitude faster then going trough an assembler.

### WASM Backend

The WASM backend was motivated by the fact that this page runs the hblang code,
but it has to run a VM inside a VM which is not as cool. Depell now supports
posts that target wasm instead of hbvm, I expect that wasm posts will be orders
of magnitude more efficient and allow more ambitious programs. (At some point I
need to add support for rendering to canvas.)

Implementing the backed was easy compared to the x64. I also added some WASM
specific optimizations so that the WASM stack is used when possible and also
some nice instruction selection for signed and unsigned loads. Although running
`wasm-opt` on the compiler output still squeezes out some opportunities its
usually in 30-40% binary size reduction.

## Improvements in the Frontend

### Partial Evaluation

NOTE: Partial evaluation happens when using `$` sign prefixed syntax. For
example `$if` requires that condition is partially evaluated. Frontend will
walk the graph of the expression and try to constant fold it into a constant,
if it encounters dependencies that can ton be folded it gives a compile time
error.

Up until recently, the partial evaluator was very messy and would easily break
on more involved code. The code managing this was just not general enough. I am
sure you know what I mean, when you design a flawed system that forces you to
code in terms of edge cases that appear indefinitely.

New evaluator is simpler while being able to evaluate more things. It also
handles capturing values in the middle of comptime mutations. This means that
code like:

```hb
main := fn(): uint {
    Val := struct{.val: uint; .b: uint; .c: uint}

    read := fn(val: Val): uint return val.val

    $val := Val.(0, 1, 2)

    value1 := read(val)

    $val.val = 3

    value2 := read(val)

    // force the val to be a comptime value
    _ = @eval(val)

    return value1 == value2
}
```

...Now works properly. This is possible becuase compiler applies smart copy on
write semantics. If comptime value was red from before, modifying it creates a
new version and modifies that. During runtime, each `read` call gets a
reference to a different static value.

## Debug Information

Compiler now emits dwarf debug information for x86_64-linux target. Gdb can now
display a stack trace that also pints to the source code. This significantly
improves development experience since you now know where the program crashed.
Evaluating the symbols is still to be desired.
