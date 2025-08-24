const hb = @import("hb");
const std = @import("std");

const max_input_size: usize = 1024 * 4;
pub export const MAX_INPUT: usize = max_input_size - 1;
pub export var INPUT: [max_input_size]u8 = undefined;
pub export var INPUT_LEN: usize = 0;

const max_output_size: usize = 1024 * 10;
pub export const MAX_OUTPUT: usize = max_output_size - 1;
pub export var OUTPUT: [max_output_size]u8 = undefined;
pub export var OUTPUT_LEN: usize = 0;

var arena: hb.utils.Arena = undefined;
var inited = false;

pub export fn fmt() void {
    errdefer unreachable;

    if (inited) {
        hb.utils.Arena.resetScratch();
        arena.reset();
    } else {
        hb.utils.Arena.initScratch(1024 * 100);
        arena = .init(1024 * 1024 * 2);
        inited = true;
    }

    const input = INPUT[0..INPUT_LEN :0];

    const ast = try hb.frontend.Ast.init(&arena, .{
        .code = input,
        .path = "",
        .ignore_errors = true,
        .mode = .legacy,
    });

    var buf = std.Io.Writer.fixed(&OUTPUT);
    try ast.fmt(&buf);
    OUTPUT_LEN = buf.end;
}

pub export fn tok() void {
    const buffer = OUTPUT[0..OUTPUT_LEN :0];

    var lexer = hb.frontend.Lexer.init(buffer, 0);
    var prev_end: usize = 0;
    while (true) {
        const token = lexer.next();

        const class = hb.frontend.Fmt.Class.fromLexeme(token.kind) orelse break;

        @memset(buffer[prev_end..token.end], @intFromEnum(class));
        prev_end = token.end;
    }
}

pub export fn minify() void {
    const buffer = OUTPUT[0..OUTPUT_LEN :0];
    OUTPUT_LEN = hb.frontend.Fmt.minify(buffer);
}
