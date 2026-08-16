build:
  cargo build --release

cli:
  cargo run -p kiruklaw-cli

test-all: (test "macros") (test "agent-loop") (test "cli")

test TEST:
  cd {{TEST}} && cargo test
