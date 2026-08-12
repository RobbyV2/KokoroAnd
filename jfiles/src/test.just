import "../common.just"

# Run all Rust tests
test:
  cd {{ROOT}} && cargo test --workspace
