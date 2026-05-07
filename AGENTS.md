# SQL Lighter: Port SQLite to Rust and Improve Performance

A multiphase project to investigate SQLite's file format, port it to Rust, and create a high-performance database engine.

With our rust version, We will use the same interface as rusqlite.

## Phase 1: Research & Discovery

**Objective:** Understand SQLite internals and existing Rust alternatives.

- [x] Clone sqlite into `sqlite/` directory (remove the .git directory)
- [x] Research existing Rust SQLite clones and wrappers → `docs/research.md`
- [x] Document competitive analysis and design decisions
- [x] Establish Rust development environment and dependencies

## Phase 2: Core Architecture Analysis

**Objective:** Deep dive into SQLite's architecture and document key components.

- [x] Analyse SQLite file format → `docs/file_format.md` (sufficient detail to implement reader/writer)
- [x] Analyse SQL dialect and supported syntax → `docs/syntax.md`
- [x] Analyse query planner architecture → `docs/planner.md`
- [x] Analyse SQL execution engine → `docs/engine.md`
- [x] Analyse plugin mechanism → `docs/plugins.md`
- [x] Investigate WAL (Write-Ahead Log) and lock file mechanisms → `docs/wal_and_locks.md`

## Phase 3: File Format Implementation

**Objective:** Implement core file I/O and data storage layer.

- [x] Implement SQLite file format reader in Rust
- [x] Implement SQLite file format writer in Rust
- [x] Create B-tree implementation for page management
- [x] Add support for pages, cells, and records
- [x] Write comprehensive tests for file format operations (25 tests, 100% passing)

## Phase 4: SQL Engine Implementation

**Objective:** Build the SQL parser, planner, and execution engine.

- [x] Implement SQL lexer and parser
- [x] Build query planner and optimizer
- [x] Implement execution engine with virtual machine
- [x] Add support for basic data types and operations
- [x] Implement indexing structures

## Phase 5: Performance Optimization

**Objective:** Optimize performance and add extensibility.

- [ ] Profile and optimize hot paths
- [ ] Implement caching and buffering strategies
- [ ] Implement plugin system
- [ ] Add SIMD optimizations where applicable
- [ ] Performance benchmarking suite

## Phase 6: Popular Wrapper Implementation

**Objective:** Create ergonomic bindings and wrappers.

- [ ] Reproduce popular SQLite wrapper (rusqlite-compatible API)
- [ ] Create high-level ORM-style API
- [ ] Add async/await support
- [ ] FFI bindings for C compatibility
- [ ] Comprehensive documentation and examples
