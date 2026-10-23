build:
  cargo build --release

cli:
  cargo run -p kiruklaw-cli

test-all: (test "macros") (test-agent-loop) (test "cli")

test TEST:
  cd {{TEST}} && cargo test

test-agent-loop:
  cargo check -p kiruklaw-agent-loop --target wasm32-unknown-unknown
  cargo test -p kiruklaw-agent-loop
  cargo test -p kiruklaw-agent-loop --target wasm32-unknown-unknown --no-run
