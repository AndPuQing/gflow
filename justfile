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

install:
    cargo install --path .

package version target:
    cd target/release && tar -czf gflow-{{version}}-{{target}}.tar.gz gflow gflowd
