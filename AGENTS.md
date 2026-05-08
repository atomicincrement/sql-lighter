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

## Phase 7: Refinement, features and optimisation (Zero-Copy Architecture) ✓ COMPLETE

- [x] Phase 7d: Replace TableStorage.page with PageRef-based reads (zero-copy table loads)
- [x] Phase 7e: Implement PageMut for write operations (direct byte writing to mmap)
- [x] Phase 7f: Replace Cell serialization with direct byte writing (pre-serialized cells_bytes)
- [x] Phase 7g: Eliminate Cell from read path (raw_cells() for byte-slice access, parse on-demand)
- [x] Phase 7h: Consolidate write operations to direct mmap bytes (single unified write path via PageMut)
- [x] Phase 7i: Remove Cell from IndexStorage (convert to pre-serialized cells_bytes, parse on-demand)
- [x] Phase 7j: Complete Cell struct removal strategy (kept for deprecated read_page(), all active paths zero-copy)

**Phase 7 Achievement:** Zero-copy database architecture fully implemented. All table and index operations use pre-serialized bytes with on-demand parsing. No intermediate Cell allocations in any hot paths. 93 tests passing, examples verified.

## Phase 8: Remaining Tasks

- [x] Complete BTree implementation: Remove Cell, follow child pages, and implement page splitting
  - Removed Cell enum from BTree struct (use pre-serialized bytes instead)
  - Implemented leaf and interior cell serialization/parsing (direct bytes)
  - Implemented page splitting on INSERT when pages overflow
  - Implemented child page navigation structure (find_insertion_pos, keys_extraction)
  - Added B-tree balancing checks (is_balanced, needs_split)
  - Added 10 comprehensive integration tests for multi-level trees
  - Goal: Fully functional B-tree infrastructure ready for multi-page support ✓
- [x] Remove DatabaseFileRead and redundant integration tests
  - DatabaseFileRead was duplicate of DatabaseFile.read_page_ref()
  - Removed all test functions from integration_read_rusqlite.rs
  - person_reverse example provides rusqlite compatibility testing
  - 99 tests passing after cleanup ✓
- [x] Phase 8b: Add Connection2 and Transaction structs for multithreading
  - Connection2 contains only Arc<DatabaseFile> for shared ownership
  - Transaction contains VirtualMachine and HashMap<u32, Box<[u8]>> for modified pages
  - Enables multiple connections to same database via Arc
  - Transaction-based execution structure in place
  - 99 tests passing, no breaking changes ✓
- [x] Phase 8c: Ensure PageRef buffer includes full 4096 bytes for page 1
  - Fixed read_page() to include file header for page 1 (bytes 0-4096)
  - Updated PageRef::header() to offset by 100 for page 1
  - Updated PageMut header methods with same offset logic
  - Updated cell pointer start calculations (108/112 vs 8/12)
  - Removed offset adjustments from cell iterators (now buffer-relative)
  - All 100 tests passing, no breaking changes ✓
- [ ] Phase 8d: Implement Transaction commit/rollback with page serialization
- [ ] Phase 8e: Check multithreading and multiprocess read/write. Check that fsync works.
- [ ] Phase 8f: Investigate and implement the WAL (Write-Ahead Log). 






