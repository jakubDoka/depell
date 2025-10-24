const max_input_size: usize = 32 * 4 * 1024;
pub export const MAX_INPUT: usize = max_input_size;
pub export var INPUT: [max_input_size]u8 = undefined;
pub export var INPUT_LEN: usize = 0;
const max_log_size: usize = 10 * 1024;
pub export var LOG_MESSAGES: [max_log_size]u8 = undefined;
pub export var LOG_MESSAGES_LEN: usize = 0;
const max_panic_size: usize = 1024;
pub export var PANIC_MESSAGE: [max_panic_size]u8 = undefined;
pub export var PANIC_MESSAGE_LEN: usize = 0;

pub export var WASM_BLOB: WasmBlob = undefined;

pub const WasmBlob = extern struct {
    ptr: [*]u8,
    len: usize,
};

pub fn panic(message: []const u8, stack_trace: ?*std.builtin.StackTrace, ret_addr: ?usize) noreturn {
    _ = stack_trace;
    _ = ret_addr;

    @memcpy(
        PANIC_MESSAGE[0..@min(message.len, max_panic_size)],
        message[0..@min(message.len, max_panic_size)],
    );
    PANIC_MESSAGE_LEN = message.len;

    PANIC_MESSAGE[PANIC_MESSAGE_LEN] = '\n';
    PANIC_MESSAGE_LEN += 1;

    @trap();
}

const std = @import("std");
const hb = @import("hb");

var arena: hb.utils.Arena = undefined;
var inited = false;

const stack_size = 1024 * 128;
var stack: [stack_size]u8 = undefined;

pub export fn compile_and_run(fuel: usize, file_count: usize, to_wasm: bool) void {
    errdefer unreachable;

    if (!inited) {
        arena = .init(1024 * 1024 * 16);
        hb.utils.Arena.initScratch(1024 * 128);
        inited = true;
    } else {
        arena.reset();
        hb.utils.Arena.resetScratch();
    }

    const FileRecord = struct {
        path: []const u8,
        source: [:0]const u8,
    };

    const files = parse_files: {
        var input_bytes = INPUT[0..INPUT_LEN];

        const files = arena.alloc(FileRecord, file_count);
        for (files) |*fr| {
            const name_len = std.mem.readInt(u16, input_bytes[0..2], .little);
            input_bytes = input_bytes[2..];
            const name = input_bytes[0..name_len];
            input_bytes = input_bytes[name_len..];

            const code_len = std.mem.readInt(u16, input_bytes[0..2], .little);
            input_bytes = input_bytes[2..];
            const code = input_bytes[0..code_len :0];
            input_bytes = input_bytes[code_len + 1 ..];

            fr.* = .{ .path = name, .source = code };
        }

        std.debug.assert(input_bytes.len == 0);

        break :parse_files files;
    };

    const KnownLoader = struct {
        files: []const FileRecord,

        pub fn load(self: *@This(), opts: hb.frontend.Ast.Loader.LoadOptions) ?hb.frontend.Types.File {
            return for (self.files, 0..) |fr, i| {
                if (std.mem.eql(u8, fr.path, opts.path)) break @enumFromInt(i);
            } else {
                return null;
            };
        }
    };

    var diagnostics = std.Io.Writer.fixed(&LOG_MESSAGES);
    defer LOG_MESSAGES_LEN = diagnostics.end;

    const asts = arena.alloc(hb.frontend.Ast, file_count);
    var known_loader = KnownLoader{ .files = files };

    for (asts, files, 0..) |*ast, fl, i| {
        ast.* = try hb.frontend.Ast.init(&arena, .{
            .diagnostics = &diagnostics,
            .path = fl.path,
            .code = fl.source,
            .current = @enumFromInt(i),
            .ignore_errors = false,
            .loader = .init(&known_loader),
        });
    }

    const types = hb.frontend.Types.init(arena, asts, &diagnostics, null);

    var backend = backend: {
        if (to_wasm) {
            const slot = types.pool.arena.create(hb.wasm.WasmGen);
            slot.* = hb.wasm.WasmGen{ .gpa = types.pool.allocator() };
            break :backend &slot.mach;
        } else {
            const slot = types.pool.arena.create(hb.hbvm.HbvmGen);
            slot.* = hb.hbvm.HbvmGen{ .gpa = types.pool.allocator() };
            break :backend &slot.mach;
        }
    };

    var threading = hb.Threading{ .single = .{ .types = types, .machine = backend } };
    const errored = hb.frontend.Codegen.emitReachable(
        hb.utils.Arena.scrath(null).arena,
        &threading,
        .{
            .has_main = true,
            .abi = if (to_wasm) .wasm else .ableos,
            .optimizations = .release,
        },
    );
    if (errored) {
        try diagnostics.print("failed due to previous errors\n", .{});
        WASM_BLOB.len = 0;
        return;
    }

    const ExecHeader = hb.hbvm.object.ExecHeader;

    const code = backend.finalizeBytes(.{
        .gpa = types.pool.allocator(),
        .builtins = .{},
        .optimizations = .{ .mode = .release },
        .files = types.line_indexes,
    }).items;

    if (to_wasm) {
        WASM_BLOB = .{ .ptr = code.ptr, .len = code.len };
        return;
    }

    const head: ExecHeader = @bitCast(code[0..@sizeOf(ExecHeader)].*);
    const stack_end = stack_size - code.len + @sizeOf(ExecHeader);
    @memcpy(stack[stack_end..], code[@sizeOf(ExecHeader)..]);

    var vm = hb.hbvm.Vm{};
    vm.ip = stack_end;
    vm.fuel = fuel * 1024;
    @memset(&vm.regs.values, 0);
    vm.regs.set(.stack_addr, stack_end);
    var ctx = hb.hbvm.Vm.SafeContext{
        .memory = &stack,
        .code_start = stack_end,
        .code_end = stack_end + @as(usize, @intCast(head.code_length)),
    };

    while (true) switch (vm.run(&ctx) catch |err| {
        try diagnostics.writeAll(@errorName(err));
        return;
    }) {
        .tx => break,
        .eca => {
            switch (vm.regs.get(.arg(0))) {
                0 => {
                    const str: [*:0]u8 = @ptrCast(&ctx.memory[@intCast(vm.regs.get(.arg(1)))]);
                    try diagnostics.writeAll(str[0..std.mem.len(str)]);
                },
                1 => {
                    const str: []const u8 = ctx.memory[@intCast(vm.regs.get(.arg(1)))..][0..@intCast(vm.regs.get(.arg(2)))];
                    try diagnostics.writeAll(str);
                },
                else => unreachable,
            }
        },

        else => unreachable,
    };

    try diagnostics.print("exit code: {}", .{vm.regs.get(.ret(0))});
}
