const max_input_size: usize = 1024 * 4;
pub export const MAX_INPUT: usize = max_input_size;
pub export var INPUT: [max_input_size]u8 = undefined;
pub export var INPUT_LEN: usize = 0;

const max_output_size: usize = 1024 * 10;
pub export const MAX_OUTPUT: usize = max_output_size;
pub export var OUTPUT: [max_output_size]u8 = undefined;
pub export var OUTPUT_LEN: usize = 0;

const hb = @import("hb");

pub export fn fmt() void {
    const input = INPUT[0..INPUT_LEN];
    _ = input; // autofix
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
            .Comment => .comment,
            .@".(", .@".[", .@".{" => .ctor,
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
            .@"[",
            .@"]",
            => .bracket,
            .@"(",
            .@")",
            .@"{",
            .@"}",
            => .paren,
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
            .@"\"",
            .@"`",
            .@"'",
            => .string,
            .@":",
            .@";",
            .@"#",
            .@"\\",
            .@",",
            => .comma,
            .@".",
            => .dot,
            .@"unterminated string" => .comment,
            .Eof => break,
        };

        @memset(buffer[prev_end..token.end], @intFromEnum(class));
        prev_end = token.end;
    }
}

pub export fn minify() void {
    const buffer = OUTPUT[0..OUTPUT_LEN :0];
    _ = buffer; // autofix
}
