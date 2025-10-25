#!/bin/bash
cd "$(dirname "$0")"
LD_LIBRARY_PATH=./target/debug:$LD_LIBRARY_PATH ./target/release/paint_spaceship "$@"
