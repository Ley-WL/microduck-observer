set -eu
base=/home/leiwu30/microduck-training/observer-baud250-20261013
cd "$base/backend-rust"
export PATH=/home/leiwu30/.cargo/bin:$PATH
export CARGO_TARGET_DIR=/home/leiwu30/microduck-training/observer-rust/target
touch src/*.rs
cargo test --locked --bin microduck-observer > "$base/test.log" 2>&1
docker run --rm -v "$base:/work" -v /home/leiwu30/.cargo:/cargo -v /home/leiwu30/.rustup:/rustup -v /home/leiwu30/microduck-training/observer-rust/target:/target -w /work/backend-rust -e CARGO_HOME=/cargo -e RUSTUP_HOME=/rustup -e CARGO_TARGET_DIR=/target -e CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main /cargo/bin/cargo build --locked --release --target aarch64-unknown-linux-gnu --bin microduck-observer > "$base/arm-build.log" 2>&1
cp /home/leiwu30/microduck-training/observer-rust/target/aarch64-unknown-linux-gnu/release/microduck-observer "$base/microduck-observer"
sha256sum "$base/microduck-observer"
