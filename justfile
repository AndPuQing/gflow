clean:
    cargo clean

test:
    cargo test

release:
    cargo build --release

install:
    cargo install --path .

test-tree:
    cargo test --bin gqueue -- --nocapture --test-threads 1
