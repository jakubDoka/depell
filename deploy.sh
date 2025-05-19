#!/bin/bash
set -e

SSH_PORT="$1"
SSH_HOST="$2"

#zig fetch --save "git+https://github.com/jakubDoka/hblang"
zig build install -Doptimize=ReleaseFast

ssh -p $SSH_PORT $SSH_HOST systemctl stop depell
scp -P $SSH_PORT ./zig-out/bin/depell $SSH_HOST:/root/depell
ssh -p $SSH_PORT $SSH_HOST systemctl start depell
