# gflow - GPU Job Scheduler

## About This Project

gflow is a lightweight, single-node job scheduler written in Rust, designed for managing GPU-intensive workloads. It provides a simple alternative to SLURM for individual researchers and small teams.

## Key Directories

```
src/
├── bin/
│   ├── gflowd/          # Daemon server (scheduler runtime, API, state management)
│   ├── gbatch/          # Job submission CLI
│   ├── gqueue/          # Job listing CLI
│   ├── gcancel/         # Job cancellation CLI
│   ├── ginfo/           # System info CLI
│   ├── gjob/            # Job details CLI
│   └── gctl/            # Control CLI
├── core/                # Core scheduler logic (job, scheduler, executor)
├── tmux.rs              # tmux integration for job execution
└── utils.rs             # Utility functions
benches/                 # Performance benchmarks
tests/                   # Integration tests
docs/                    # User documentation (mdBook)
```

## Standards & Conventions

### Code Style
- Follow Rust standard formatting (`cargo fmt`)
- Run clippy before committing (`cargo clippy`)
- Pre-commit hooks enforce formatting and linting

### Testing
- Unit tests in module files
- Integration tests in `tests/` directory
- Run all tests before major changes: `cargo test`
- Run benchmarks for performance-critical changes: `cargo bench`

### Performance Claims
**IMPORTANT**: Never make specific performance claims (e.g., "2x faster", "50% smaller") without backing them up with actual benchmark results.

When making performance-related changes:
1. Write benchmarks using the `criterion` crate in `benches/`
2. Run benchmarks and collect actual data
3. Document the test methodology (e.g., "tested with 10/100/1000 jobs")
4. Use measured results in commit messages and documentation
5. Include benchmark code in the same commit

Example of proper performance documentation:
```
Benchmark results (tested with 10/100/1000 jobs):
- File size reduction: ~70% smaller than JSON
- Serialization speed: ~2.3x faster than JSON
- Deserialization speed: ~2.1x faster than JSON
```

### Commit Messages
- Use conventional commits format: `feat:`, `fix:`, `perf:`, `docs:`, etc.
- Do NOT add co-author information unless explicitly requested
- Include issue references: `Closes #123`

## Common Commands

```bash
# Build and run
cargo build --release
cargo run --bin gflowd

# Testing
cargo test                              # Run all tests
cargo test --bin gflowd                 # Run gflowd tests only
cargo bench --bench serialization_bench # Run specific benchmark

# Development
cargo fmt                               # Format code
cargo clippy                            # Run linter
cargo check                             # Quick compile check

# Documentation
cd docs && mdbook serve                 # Serve documentation locally
```

## Architecture Notes

### State Persistence
- Uses MessagePack binary format (`state.msgpack`) for efficient serialization
- Automatic migration from legacy JSON format (`state.json`)
- Journal-based recovery mode for corrupted state files
- Atomic writes using temp file + rename pattern

### Job Execution
- Jobs run in tmux sessions for persistence
- GPU allocation managed via NVML
- Memory limits enforced at submission time
- Dependency chains supported with auto-cancellation

### API Design
- REST API via Axum framework
- Read-only mode when state is unwritable
- Event-driven architecture for monitoring and scheduling

## Project-Specific Guidelines

### When Adding New Features
1. Check if similar functionality exists in the codebase
2. Follow existing patterns (e.g., CLI structure, error handling)
3. Update both English and Chinese documentation
4. Add tests for new functionality

### When Optimizing Performance
1. Profile first to identify actual bottlenecks
2. Write benchmarks before and after optimization
3. Document the improvement with real numbers
4. Consider backward compatibility

### When Modifying State Format
1. Add migration logic in `src/core/migrations.rs`
2. Increment `CURRENT_VERSION` constant
3. Test migration from previous versions
4. Update recovery mode handling if needed
