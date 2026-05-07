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

## Phase 5: Popular Wrapper Implementation

**Objective:** Create ergonomic bindings and wrappers.

- [x] Look at the example on https://github.com/rusqlite/rusqlite
- [x] Implement Connection with the same interface as rusqlite (6a)
- [x] Implement open_in_memory opening a sqlite storage without a file but still using mmap (6a)
- [x] Implement parameter substitution with ?1, ?2 etc in SQL. constrain parameters to literals (6b)
- [x] Implement execute for CREATE TABLE only (6b)
- [x] Implement execute for INSERT INTO only (6b)
- [x] Implement execute for SELECT only (6b)
- [x] Create example mirroring rusqlite's person example (examples/person.rs)

## Phase 6: Making it work.

Claude: please do not add to this, just tick the boxes!

- [x] Create an Error and Result type that is the same as rusqlite.
- [x] Create a Params trait that is the same as rusqlite::Params and implement it for the same types in params.rs
- [x] Remove set_param() and make execute() use Params trait directly
- [x] Implement prepare, Statement, and query_map methods
- [x] Use the Btree storage from file_format for VirtualMachine.
- [x] Split the person example. Write the table using rusqlite and read it with sql-lighter.
- [x] Implement the indices using the BTree storage.
- [x] Reverse the split person example, write the table using sql-lighter and read using rusqlite.

## Phase 7: Refinement, features and optimisation.

- [ ] Use zero copy everywhere in DatabaseFileRead. Create PageRef on demand from the memory map. Retire page_cache.
- [ ] Implement the pre-WAL classic sqlite writing using write(). Update the memory map when the file changes size.
- [ ] Check multithreading and multiprocess read/write. Check that fsync works by creating several Connection instances.
- [ ] Investigate the WAL.





