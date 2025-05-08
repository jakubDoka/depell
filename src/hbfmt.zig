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
    });

    var allc = std.heap.FixedBufferAllocator.init(&OUTPUT);
    var buf = try std.ArrayList(u8).initCapacity(allc.allocator(), max_input_size);

    try ast.fmt(&buf);

    OUTPUT_LEN = buf.items.len;
}

pub export fn tok() void {
    const Class = enum(u8) {
        blank,
        comment,
        keyword,
        identifier,
        directive,
        number,
        string,
        op,
        assign,
        paren,
        bracket,
        colon,
        comma,
        dot,
        ctor,
    };

    const buffer = OUTPUT[0..OUTPUT_LEN :0];

    var lexer = hb.frontend.Lexer.init(buffer, 0);
    var prev_end: usize = 0;
    while (true) {
        const token = lexer.next();

        const class: Class = switch (token.kind) {
            .ty_never,
            .ty_void,
            .ty_bool,
            .ty_u8,
            .ty_u16,
            .ty_u32,
            .ty_u64,
            .ty_uint,
            .ty_i8,
            .ty_i16,
            .ty_i32,
            .ty_i64,
            .ty_int,
            .ty_f32,
            .ty_f64,
            .ty_type,
            .Ident,
            .@"$",
            ._,
            => .identifier,
            .@"@CurrentScope",
            .@"@use",
            .@"@TypeOf",
            .@"@as",
            .@"@int_cast",
            .@"@size_of",
            .@"@align_of",
            .@"@bit_cast",
            .@"@ecall",
            .@"@embed",
            .@"@inline",
            .@"@len_of",
            .@"@kind_of",
            .@"@name_of",
            .@"@is_comptime",
            .@"@Any",
            .@"@error",
            .@"@ChildOf",
            .@"@target",
            .@"@int_to_float",
            .@"@float_to_int",
            .@"@float_cast",
            => .directive,
            .@"fn",
            .@"return",
            .@"defer",
            .die,
            .@"if",
            .@"$if",
            .@"else",
            .match,
            .@"$match",
            .@"$loop",
            .loop,
            .@"break",
            .@"continue",
            .@"enum",
            .@"union",
            .@"struct",
            .@"align",
            .null,
            .idk,
            => .keyword,
            .true,
            .false,
            .BinInteger,
            .OctInteger,
            .DecInteger,
            .HexInteger,
            .Float,
            => .number,
            .@"!",
            .@"&",
            .@"*",
            .@"+",
            .@"<",
            .@">",
            .@"%",
            .@"|",
            .@"~",
            .@"^",
            .@"/",
            .@"..",
            .@"<<",
            .@"=>",
            .@">>",
            .@"-",
            .@"?",
            => .op,
            .@"=",
            .@"!=",
            .@"+=",
            .@"-=",
            .@"*=",
            .@"/=",
            .@"%=",
            .@"|=",
            .@"^=",
            .@"&=",
            .@"<<=",
            .@">>=",
            .@":=",
            .@"<=",
            .@"==",
            .@">=",
            => .assign,
            .Comment => .comment,
            .@".(", .@".[", .@".{" => .ctor,
            .@"[", .@"]" => .bracket,
            .@"(", .@")", .@"{", .@"}" => .paren,
            .@"\"", .@"`", .@"'" => .string,
            .@":", .@";", .@"#", .@"\\", .@"," => .comma,
            .@"." => .dot,
            .@"unterminated string" => .comment,
            .Eof => break,
        };

        @memset(buffer[prev_end..token.end], @intFromEnum(class));
        prev_end = token.end;
    }
}

pub export fn minify() void {
    const buffer = OUTPUT[0..OUTPUT_LEN :0];
    OUTPUT_LEN = hb.frontend.Fmt.minify(buffer);
}
