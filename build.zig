const std = @import("std");

pub fn buildWasm(
    b: *std.Build,
    hb: *std.Build.Module,
    comptime name: []const u8,
    exports: []const []const u8,
) *std.Build.Step.InstallFile {
    const exe = b.addExecutable(.{
        .name = name,
        .root_source_file = b.path("src/" ++ name ++ ".zig"),
        .target = hb.resolved_target,
        .optimize = hb.optimize.?,
    });

    exe.root_module.addImport("hb", hb);

    exe.root_module.export_symbol_names = exports;
    exe.entry = .disabled;

    var out = exe.getEmittedBin();

    if (hb.optimize == .ReleaseSmall) {
        const wasm_opt = b.addSystemCommand(&.{ "wasm-opt", "-Oz", "--enable-bulk-memory", "--enable-nontrapping-float-to-int" });
        wasm_opt.addFileArg(out);
        wasm_opt.addArg("-o");
        out = wasm_opt.addOutputFileArg(name ++ ".wasm");
    }

    const gzip = b.addSystemCommand(&.{"gzip"});
    gzip.addArg("-c");
    gzip.addFileArg(out);
    out = gzip.captureStdOut();

    return b.addInstallBinFile(out, name ++ ".wasm.gz");
}

pub fn build(b: *std.Build) !void {
    // const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const run = b.step("run", "run the depell server");

    const build_depell = b.addSystemCommand(&.{ "cargo", "build" });

    build_depell: {
        var out_bin: []const u8 = "target/debug/depell";
        if (optimize == .ReleaseFast) {
            build_depell.addArgs(&.{
                "--release",
                "--features",
                "tls",
                "--target",
                "x86_64-unknown-linux-musl",
            });
            out_bin = "target/x86_64-unknown-linux-musl/release/depell";
        }

        const out = b.addInstallBinFile(b.path(out_bin), "depell");
        out.step.dependOn(&build_depell.step);
        b.getInstallStep().dependOn(&out.step);

        const runner = b.addSystemCommand(&.{out_bin});
        runner.step.dependOn(b.getInstallStep());
        run.dependOn(&runner.step);

        break :build_depell;
    }

    compress_assets: {
        const assets: []const []const u8 = &.{ "index.css", "index.js" };

        inline for (assets) |ass| {
            const gzip = b.addSystemCommand(&.{"gzip"});
            gzip.addArg("-c");
            gzip.addFileArg(b.path("src/" ++ ass));
            const out = gzip.captureStdOut();
            build_depell.step.dependOn(&b.addInstallFile(out, "static/" ++ ass ++ ".gz").step);
        }

        break :compress_assets;
    }

    build_wasm: {
        const wasm_target = b.resolveTargetQuery(.{ .cpu_arch = .wasm32, .os_tag = .freestanding });
        const hblang = b.dependency("hblang", .{
            .target = wasm_target,
            .optimize = if (optimize == .ReleaseFast) .ReleaseSmall else optimize,
        });

        const hb = hblang.module("hb");

        build_depell.step.dependOn(&buildWasm(b, hb, "hbc", &.{
            "MAX_INPUT",
            "INPUT",
            "INPUT_LEN",

            "LOG_MESSAGES",
            "LOG_MESSAGES_LEN",

            "compile_and_run",
            "__stack_pointer",
        }).step);

        build_depell.step.dependOn(&buildWasm(b, hb, "hbfmt", &.{
            "MAX_INPUT",
            "INPUT",
            "INPUT_LEN",

            "MAX_OUTPUT",
            "OUTPUT",
            "OUTPUT_LEN",

            "fmt",
            "tok",
            "minify",
            "__stack_pointer",
        }).step);

        break :build_wasm;
    }

    render_markdown: {
        const mumd_render = b.addExecutable(.{
            .name = "mumd_render",
            .optimize = .Debug,
            .target = b.graph.host,
        });
        mumd_render.root_module.addIncludePath(b.path("vendored/mumd"));
        mumd_render.root_module.addCSourceFile(.{ .file = b.path("vendored/mumd/example.c") });
        mumd_render.linkLibC();

        const dir = try std.fs.cwd().openDir("src/static-pages/", .{ .iterate = true });
        var iter = dir.iterate();

        while (try iter.next()) |f| {
            std.debug.assert(f.kind == .file);
            std.debug.assert(std.mem.endsWith(u8, f.name, ".md"));

            const translate = b.addRunArtifact(mumd_render);
            translate.addFileArg(b.path(try std.fs.path.join(b.allocator, &.{ "src/static-pages/", f.name })));
            var out = translate.captureStdOut();
            var ext: []const u8 = ".html";

            if (false and optimize == .ReleaseFast) {
                const gzip = b.addSystemCommand(&.{"gzip"});
                gzip.addArg("-c");
                gzip.addFileArg(out);
                out = gzip.captureStdOut();
                ext = ".html.gz";
            }

            const name = try std.mem.replaceOwned(u8, b.allocator, f.name, ".md", ext);
            const sub_path = try std.fs.path.join(b.allocator, &.{ "static", name });
            build_depell.step.dependOn(&b.addInstallFile(out, sub_path).step);
        }

        break :render_markdown;
    }
}
