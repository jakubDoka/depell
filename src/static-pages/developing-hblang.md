# The journey to an optimizing compiler

It's been years since I was continuously trying to make a compiler to implement language of my dreams. Problem was though, that I wanted something similar to Rust, which if you did not know, `rustc` far exceeded the one million lines of code mark some time ago, so implementing such language would take me years if not decades, but I still tired it.

Besides being extremely ambitions, the problem with my earliest attempts at making a compiler, is that literally nobody, not even me, was using the language, and so retroactively I am confident, what I implemented was a complex test-case implementation, and not a compiler. I often fall into a trap of implementing edge cases instead of an algorithm that would handle not only the very few thing the tests do but also all the other stuff that users of the language would try.

Another part of why I was failing for so long, is that I did the hardest thing first without understanding the core concepts involved in translating written language to IR, god forbid an assembly. I wasted a lot of time like this, but at least I learned Rust well. At some point I found a job where I started developing a decentralized network and that fully drawn me away from language development.

## Completely new approach

At some point the company I was working for started having financial issues and they were unable to pay me. During that period, I discovered that my love for networking was majorly fueled by the monetary gains associated with it. I burned out, and started to look for things to do with the free time.

One could say timing was perfect because [`ableos`](https://git.ablecorp.us/AbleOS/ableos) was desperately in need of a sane programming language that compiles to the home made VM ISA used for all software ran in `ableos`, but there was nobody crazy enough to do this. I got terribly nerd sniped, tho I don't regret it. Process of making a language for `ableos` was completely different. Firstly, it needed to be done asap, the lack of a good language blocked everyone form writing drivers for `ableos`, secondly, the moment the language is at least a little bit usable, people other then me started using it, and lastly, the ISA, the language compiles to, is very simple to emit, understand, and run.

### Urgency is a bliss

I actually managed to make the language somewhat work in one week, mainly because my mind set changed. I no longer spent a lot of time designing syntax for elegance, I designed it for ease of parsing, meaning I can spent minimal effort implementing the parser, and fully focus on the hard problem of translating AST to instructions. Surprisingly, making everything an expression and not enforcing any arbitrary rules, makes the code you can write incredibly flexible and (most) people love it. One of the decisions I made to save time (or maybe it was an accident) was to make `,;` not enforced, meaning, you are allowed to write delimiters in lists but, as long as it does not change the intent of the code, you can leave them out. In practice, you actually don't need semicolons, unless the next line starts with something sticky like `*x`, in that case you put a semicolon on the previous line to tell the parser where the current expression ends.

### Only the problem I care about

Its good to note that writing a parser is no longer interesting for me. I wrote many parsers before and writing one no longer feel rewarding, but more like a chore. The real problem I was excited about was translating AST to instructions, I always ended up overcomplicating this step wit edge cases for every possible scenario that can happen in code, for which there are infinite. But why did I succeed this time? Well all the friction related to getting something that I can execute was so low, I could iterate quickly and realize what I am doing wrong before I burn out. In a week I managed to understand what I was failing to do for years, partly because of all the previous suffering, but mainly because it was so easy to pivot and try new things. And so I managed to make my first single pass compiler, and people immediately started using it.

### Don't implement features nobody asked for

Immediately after someone else then me wrote something in `hb` stuff started breaking, over the course of a month I kept fixing bugs and adding new features just fine, and more people started to use the language. All was good and well until I looked into the code. It was incredibly cursed, full of tricks to work around the compiler not doing any optimizations. At that moment I realized the whole compiler after parser needs to be rewritten, I had to implement optimizations, otherwise people wont be able to write readable code that runs fast. All of the features I have added up until now, were a technical dept now. Unless they are all working with optimizations, existing code would be rendered useless. Yes, if feature exists, be sure as hell it will be used.

It took around 4 months to reimplement everything make the optimal code look like what you are used to in other languages. I am really thankful for [sea of nodes](https://github.com/SeaOfNodes), and all the amazing work Cliff Click and others do to make demystify optimizers. It would have taken much longer for me to figure all the principles, without the exhaustive [tutorial](https://github.com/SeaOfNodes/Simple?tab=readme-ov-file).

## How my understanding of optimizations changed

### Optimizations allow us to scale software

I need to admit, before writing a single pass compiler and later upgrading it to optimizing one, I thought optimizations only affect the quality of final assembly emitted by the compiler. It never occur to me that what the optimizations actually do, is reduce the impact of how you decide to write the code. In a single pass compiler (with zero optimizations), the machine code reflects:

- order of operations as written in code
- whether the value was stored in intermediate locations
- exact structure of the control flow and at which point the operations are placed
- how many times is something recomputed
- operations that only help to convey intent for the reader of the source code
- and more I can't think of...

If you took some code you wrote and then modified it to obfuscate these aspects (in reference to the original code), you would to a subset of what optimizing compiler does. Of course, a good compiler would try hard to improve the metrics its optimizing for, it would:

- reorder operations to allow the CPU to parallelize them
- remove needless stores, or store values directly to places you cant express in code
- pull operations out of the loops and into the branches (if it can)
- find all common sub-expressions and compute them only once
- fold constants as much as possible and use obscure tricks to replace slow instructions if any of the operands are constant
- and more...

In the end, compiler optimizations try to reduce correlation between how the code happens to be written and how well it performs, which is extremely important when you want humans to read the code.

### Optimizing compilers know more then you

Optimizing code is a search problem, an optimizer searches the code for patterns that can be rewritten so something more practical for the computer, while preserving the observable behavior of the program. This means it needs enough context about the code to not make a mistake. In fact, the optimizer has so much context, it is able to determine your code is useless. But wait, didn't you write the code because you needed it to do something? Maybe your intention was to break out of the loop, but the optimizer looked at the code and said, "great, we are so lucky that this integer is always small enough to miss this check by one, DELETE", and then he goes "jackpot, since this loop is now infinite, we don't need this code after it, DELETE". Notice that the optimizer is eager to delete dead code, it did not ask you "Brah, why did you place all your code after an infinite loop?". This is just an example, there are many more cases where modern optimizers just delete all your code because they proven it does something invalid without running it.

Its stupid but its the world we live in, optimizers are usually a black box you import and feed it the code in a format they understand, they then proceed to optimize it, and if they find a glaring bug they wont tell you, they will just molest the code in unspecified ways and spit out whats left. Before writing an optimizer, I did no know this can happen and I did not know this is a problem I pay for with my time, spent figuring out why noting is happening when I run the program.

But wait its worse! Since optimizers wont ever share the fact you are stupid, we end up with other people painstakingly writing complex linters, that will do a shitty job detecting things that matter, and instead whine about style and other bullcrap (and they suck even at that). If the people who write linters and people who write optimizers swapped the roles, I would be ranting about optimizers instead.

And so, this is the area where I want to innovate, lets report the dead code to the frontend, and let the compiler frontend filter out the noise and show relevant information in the diagnostics. Refuse to compile the program if you `i /= 0`. Refuse to compile if you `arr[arr.len]`. This is the level of stupid optimizer sees, once it normalizes your code, but proceeds to protect your feelings. My goal so for hblang to relay this to you as much as possible. If we can query for optimizations, we can query for bugs too.
