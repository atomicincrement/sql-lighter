# SQL Lighter: Port SQLite to Rust and Improve Performance

A multiphase project to investigate SQLite's file format, port it to Rust, and create a high-performance database engine.

## Phase 1: Research & Discovery

**Objective:** Understand SQLite internals and existing Rust alternatives.

- [x] Clone sqlite into `sqlite/` directory (remove the .git directory)
- [x] Research existing Rust SQLite clones and wrappers → `docs/research.md`
- [x] Document competitive analysis and design decisions
- [x] Establish Rust development environment and dependencies

## Phase 2: Core Architecture Analysis

**Objective:** Deep dive into SQLite's architecture and document key components.

- [ ] Analyse SQLite file format → `docs/file_format.md` (sufficient detail to implement reader/writer)
- [ ] Analyse SQL dialect and supported syntax → `docs/syntax.md`
- [ ] Analyse query planner architecture → `docs/planner.md`
- [ ] Analyse SQL execution engine → `docs/engine.md`
- [ ] Analyse plugin mechanism → `docs/plugins.md`

## Phase 3: File Format Implementation

**Objective:** Implement core file I/O and data storage layer.

- [ ] Implement SQLite file format reader in Rust
- [ ] Implement SQLite file format writer in Rust
- [ ] Create B-tree implementation for page management
- [ ] Add support for pages, cells, and records
- [ ] Write comprehensive tests for file format operations

## Phase 4: SQL Engine Implementation

**Objective:** Build the SQL parser, planner, and execution engine.

- [ ] Implement SQL lexer and parser
- [ ] Build query planner and optimizer
- [ ] Implement execution engine with virtual machine
- [ ] Add support for basic data types and operations
- [ ] Implement indexing structures

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
