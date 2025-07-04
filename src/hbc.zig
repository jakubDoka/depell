const max_input_size: usize = 32 * 4 * 1024;
pub export const MAX_INPUT: usize = max_input_size;
pub export var INPUT: [max_input_size]u8 = undefined;
pub export var INPUT_LEN: usize = 0;
const max_log_size: usize = 10 * 1024;
pub export var LOG_MESSAGES: [max_log_size]u8 = undefined;
pub export var LOG_MESSAGES_LEN: usize = 0;

const std = @import("std");
const hb = @import("hb");

var arena: hb.utils.Arena = undefined;
var inited = false;

const stack_size = 1024 * 128;
var stack: [stack_size]u8 = undefined;

pub export fn compile_and_run(fuel: usize, file_count: usize) void {
    errdefer unreachable;

    if (!inited) {
        arena = .init(1024 * 1024 * 2);
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

    var logs: []u8 = &LOG_MESSAGES;
    const LogWriter = std.io.GenericWriter(*[]u8, error{OutOfMemory}, struct {
        fn wfn(ctx: *[]u8, data: []const u8) error{OutOfMemory}!usize {
            if (ctx.len < data.len) return error.OutOfMemory;
            @memcpy(ctx.*[0..data.len], data);
            ctx.* = ctx.*[data.len..];
            LOG_MESSAGES_LEN += data.len;
            return data.len;
        }
    }.wfn);
    const diagnostics = (LogWriter{ .context = &logs }).any();

    const asts = arena.alloc(hb.frontend.Ast, file_count);
    var known_loader = KnownLoader{ .files = files };

    for (asts, files, 0..) |*ast, fl, i| {
        ast.* = try hb.frontend.Ast.init(&arena, .{
            .diagnostics = diagnostics,
            .path = fl.path,
            .code = fl.source,
            .current = @enumFromInt(i),
            .ignore_errors = false,
            .loader = .init(&known_loader),
        });
    }

    const types = hb.frontend.Types.init(arena, asts, diagnostics);

    var backend = backend: {
        const slot = types.pool.arena.create(hb.hbvm.HbvmGen);
        slot.* = hb.hbvm.HbvmGen{ .gpa = types.pool.allocator() };
        break :backend hb.backend.Machine.init("hbvm-ableos", slot);
    };

    const errored = hb.frontend.Codegen.emitReachable(
        hb.utils.Arena.scrath(null).arena,
        types,
        .ableos,
        backend,
        .all,
        true,
        .{},
    );
    if (errored) {
        try diagnostics.print("failed due to previous errors\n", .{});
        return;
    }

    if (false) {
        backend.disasm(diagnostics, .no_color);
        return;
    }

    const ExecHeader = hb.hbvm.object.ExecHeader;

    const code = backend.finalizeBytes(.{ .gpa = types.pool.allocator(), .builtins = .{} }).items;

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
                else => unreachable,
            }
        },

        else => unreachable,
    };

    try diagnostics.print("exit code: {}", .{vm.regs.get(.ret(0))});
}
