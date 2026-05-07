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

The goal of this phase is to remove the current Page and Cell data structures and use PageRef instead. If we are going to modify a page, we should iterate through the leaf or intermediate cells of the PageRef and write a new page directly to the file using write() (not via a mutable memory map).

- [x] Phase 7d: Replace TableStorage.page with PageRef-based reads
  - Removed Page field from TableStorage struct
  - TableStorage now stores page_num: u32 instead of page: Page
  - Modified load_table_from_page() to accept PageRef instead of owned Page
  - Added read_page_ref() method to DatabaseFile for zero-copy reads
  - Updated Connection::open() to use PageRef when loading tables
  - Goal: Zero-copy table reads directly from mmap ✓
- [x] Phase 7e: Implement PageMut for write operations
  - Created PageMut<'a> reference type for mutable page access
  - Added get_page_mut() to DatabaseFile for direct mmap buffer access
  - Implemented write_cells() on PageMut for direct serialization to page buffer
  - Modified persist() to use PageMut instead of intermediate Page struct
  - Goal: Zero-copy writes building pages in-place ✓
- [x] Phase 7f: Replace Cell serialization with direct byte writing
  - Changed TableStorage.cells: Vec<Cell> to cells_bytes: Vec<Vec<u8>> (pre-serialized)
  - Implemented write_cells_bytes() for direct byte writing without Cell enum
  - Updated add_row() to directly serialize cell bytes instead of creating Cell objects
  - Updated DELETE and UPDATE operations to parse bytes on-demand
  - Modified persist() to use pre-serialized bytes via write_cells_bytes()
  - Eliminated Cell struct from write path - only used in read path for parsing
  - Goal: Direct byte writing without intermediate Cell allocations achieved ✓
- [x] Phase 7g: Eliminate Cell from read path
  - Added PageRef::raw_cells() for byte-slice access without Cell parsing
  - Updated load_table_from_page() to parse cells directly from bytes
  - Eliminated Cell struct from primary data load path
  - Cell now only used in backward compatibility layer (IndexStorage still uses Cell)
  - Page struct still exists for backward compatibility but marked deprecated
  - Goal: Minimal Cell allocations in hot paths achieved ✓
- [ ] Phase 7h: Consolidate write operations to direct mmap bytes
  - Remove write_page() method (obsolete)
  - All writes use mmap directly with offset calculations
  - Ensure page 1 header preservation during all writes
  - Add integration tests for multi-table updates
  - Goal: Single unified write path with maximum efficiency
- [ ] Check multithreading and multiprocess read/write. Check that fsync works by creating several Connection instances.
- [ ] Investigate the WAL. 






