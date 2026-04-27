clean:
    cargo clean

test:
    cargo test

web:
    bun install --cwd web --frozen-lockfile
    bun run --cwd web lint
    bun run --cwd web build

release:
    bun run --cwd web build
    cargo build --release

minimal:
    bun run --cwd web build
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
