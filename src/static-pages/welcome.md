## Welcome to depell

Depell (dependency hell) is a simple "social" media site, except that all you can post is [hblang](https://git.ablecorp.us/AbleOS/holey-bytes) code. Instead of likes you run the program, and instead of mentions you import the program as dependency. Run counts even when ran indirectly.

The backend only serves the code and frontend compiles and runs it locally. All posts are immutable.

## Security?

All code runs in WASM (inside a holey-bytes VM until hblang compiles to wasm) and is controlled by JavaScript. WASM
cant do any form of IO without going trough JavaScript so as long as JS import does not allow wasm to execute
arbitrary JS code, WASM can act as a container inside the JS.
