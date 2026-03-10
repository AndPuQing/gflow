clean:
    cargo clean

test:
    cargo test

release:
    cargo build --release

minimal:
    cargo build --profile minimal-size --bins

install:
    cargo install --path .

test-tree:
    cargo test --lib multicall::gqueue::commands::list::tests:: -- --nocapture --test-threads 1

bench-regression-list:
    python3 scripts/benchmark_regression.py list

bench-regression-baseline name="local":
    python3 scripts/benchmark_regression.py save-baseline --name {{name}}

bench-regression-compare name="local":
    python3 scripts/benchmark_regression.py compare --name {{name}}
