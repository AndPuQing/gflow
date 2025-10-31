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

reset-md: install
    gflowd down
    gflowd up

doc: reset-md
    cd docs && mdbook serve
