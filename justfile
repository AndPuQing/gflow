clean:
    cargo clean

gflow *ARGS:
    cargo run --bin gflow -- {{ARGS}}

gflowd *ARGS:
    cargo run --bin gflowd -- {{ARGS}}

test:
    cargo test

release:
    cargo build --release