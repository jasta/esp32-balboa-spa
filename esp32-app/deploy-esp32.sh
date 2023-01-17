#!/bin/sh

export RUSTUP_TOOLCHAIN=esp
cargo run --bin topside_panel --target=xtensa-esp32-espidf
