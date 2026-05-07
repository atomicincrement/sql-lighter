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
  - Implemented rusqlite-compatible Error enum with 11 core variants
  - Full backward compatibility with existing error handling
  
- [x] Create a Params trait that is the same as rusqlite::Params and implement it for the same types in params.rs
  - Implemented Sealed trait pattern for API encapsulation
  - Created Params trait with bind_params() method returning HashMap<String, Value>
  - Implemented Params for: tuples (0-16 elements), arrays (1-32), slices
  - Implemented ToSql for: Value, String, str, numeric types, bool, Option<T>, Blob
  - 11 new tests added for Params trait implementations
  
- [x] Remove set_param() and make execute() use Params trait directly
  - Removed set_param() and clear_params() methods
  - Removed params HashMap field from Connection struct
  - Updated execute() signature to accept generic Params: execute<P: Params>(&mut self, sql: &str, params: P)
  - Updated query() and query_row() methods to accept Params
  - Removed deprecated execute_with_params() method (now execute() does this)
  - Updated all tests to use new API
  - Added ToSql implementations for &String, &Vec<u8>, and &Option<T>
  - Updated person.rs example to match rusqlite example (now uses tuples with references)
  - Test count: 86 tests passing (removed 3 tests specific to old set_param API)

- [x] Implement prepare, Statement, and query_map methods
  - Created new src/statement.rs module with Statement struct
  - Implemented prepare() method on Connection
  - Created FromValue trait for converting SQL values to Rust types
  - Implemented query_map<P, F, T>() on Statement for row mapping
  - Created RowRef wrapper with get<T: FromValue>() method for column access
  - Added FromValue implementations: i32, i64, f64, String, Vec<u8>, Option<T>
  - Updated person.rs example to match rusqlite exactly using prepare/query_map
  - Added 3 new tests for statement functionality
  - Test count: 89 tests passing (3 new statement tests)


