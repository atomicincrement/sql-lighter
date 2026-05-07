# Research: Rust SQLite Ecosystem & Alternatives

## Executive Summary

This document surveys the existing Rust SQLite ecosystem, including wrappers, clones, and alternative embedded database engines. The goal is to understand the competitive landscape and inform design decisions for SQL Lighter.

## Existing Rust SQLite Implementations

### 1. Rusqlite
**Type:** FFI binding  
**URL:** https://github.com/rusqlite/rusqlite  
**Status:** Mature, actively maintained

**Strengths:**
- Battle-tested C FFI bindings to native SQLite
- Simple API for basic operations
- Small dependency footprint
- Widely used in production

**Weaknesses:**
- Tied to C SQLite binary (not portable)
- No async/await support (blocking only)
- Limited type safety
- Requires SQLite library installation

**Architecture:**
- Thin wrapper around SQLite C API
- Connection pooling via external libraries (r2d2)
- Manual prepared statement management

### 2. SQLx
**Type:** SQL toolkit with async support  
**URL:** https://github.com/launchbr/sqlx  
**Status:** Actively maintained, production-ready

**Strengths:**
- Compile-time query verification
- Full async/await support
- Cross-database support (SQLite, PostgreSQL, MySQL, MSSQL)
- Type-safe query results
- Macros for query checking at compile time

**Weaknesses:**
- Heavier API surface
- Still uses rusqlite under the hood for SQLite
- Learning curve for macro system

**Architecture:**
- Generic database abstraction layer
- Optional compile-time verification
- Connection pool management built-in

### 3. Diesel
**Type:** ORM framework  
**URL:** https://github.com/diesel-rs/diesel  
**Status:** Mature, actively maintained

**Strengths:**
- Powerful query DSL
- Type-safe schema definitions
- Query builder approach
- Migration support
- SQLite support (among others)

**Weaknesses:**
- Blocking only (no async)
- Steep learning curve
- Opinionated architecture
- Still uses C SQLite under the hood

**Architecture:**
- Query builder pattern
- Schema code generation
- Transaction support

### 4. Sea-ORM
**Type:** ORM framework  
**URL:** https://github.com/SeaQL/sea-orm  
**Status:** Actively maintained, growing adoption

**Strengths:**
- Async-first design
- Type-safe queries
- Multiple database support
- Entity relationship modeling
- Good documentation

**Weaknesses:**
- Newer (less battle-tested)
- Smaller community than Diesel
- Still uses underlying database drivers

**Architecture:**
- Entity-based models
- Async query interface
- Migration system

### 5. LibSQL
**Type:** SQLite fork/alternative  
**URL:** https://github.com/libsql/libsql  
**Status:** Emerging, actively developed by Turso

**Strengths:**
- Modern SQLite fork with extensions
- Better performance in some scenarios
- Rust-first design potential
- Wasm support
- Replication features

**Weaknesses:**
- Still relatively new
- Smaller ecosystem
- Not pure Rust

**Architecture:**
- Based on SQLite source
- Added features: replication, encryption
- Multiple client options

### 6. Sled
**Type:** Pure Rust embedded database  
**URL:** https://github.com/spacejam/sled  
**Status:** Stable but limited active development

**Strengths:**
- Pure Rust implementation
- No external dependencies
- ACID transactions
- High performance
- Simple key-value interface

**Weaknesses:**
- Not SQL-based (key-value store)
- Limited query capabilities
- Smaller ecosystem than SQLite

**Architecture:**
- Lock-free B+ tree
- Zero-copy reads
- Transaction log

### 7. RocksDB Rust Bindings
**Type:** FFI to RocksDB  
**URL:** https://github.com/rust-rocksdb/rust-rocksdb  
**Status:** Actively maintained

**Strengths:**
- High performance
- Column family support
- Mature underlying C++ library
- Good for write-heavy workloads

**Weaknesses:**
- Not SQL-based
- No transactions across column families
- Complex API
- Larger resource footprint

## Pure Rust SQL Engine Implementations

### DuckDB
**Type:** Alternative SQL engine  
**Status:** Actively developed, OLAP-focused

**Strengths:**
- Vectorized query execution
- Excellent analytical performance
- Multiple file format support
- Good Rust support

**Weaknesses:**
- OLAP optimized (not OLTP)
- Heavier memory footprint
- Different architectural goals than SQLite

## Competitive Analysis

### Performance Characteristics

| Implementation | Type | Speed | Memory | Portability | Dependencies |
|---|---|---|---|---|---|
| SQLite (C) | Native | High | Low | Excellent | None |
| Rusqlite | FFI Binding | High | Low | Good | SQLite C lib |
| SQLx | Toolkit | High | Low | Excellent | Database libs |
| Diesel | ORM | Medium | Medium | Good | Database libs |
| Sled | Pure Rust | Very High | Medium | Excellent | None |
| DuckDB | SQL Engine | Very High (OLAP) | Medium-High | Good | None |

### Use Case Suitability

**Rusqlite:** Best for simple CRUD operations with minimal Rust overhead

**SQLx:** Best for type-safe applications with potential multi-database support

**Diesel:** Best for complex domain models with relationships

**Sea-ORM:** Best for async web applications with rich entities

**Sled:** Best for high-performance key-value storage without SQL

**DuckDB:** Best for analytical queries and data processing

## Design Decisions for SQL Lighter

### Proposed Approach

SQL Lighter should differentiate itself by:

1. **Pure Rust Implementation**
   - No C dependencies
   - Portable to any platform Rust supports (including WASM)
   - Better memory safety guarantees

2. **SQLite Compatibility**
   - File format compatible with SQLite
   - Same SQL dialect
   - Could eventually replace SQLite as a drop-in

3. **Performance Focus**
   - Profile and optimize hot paths
   - SIMD optimizations where applicable
   - Better cache locality
   - Modern memory management

4. **Modern Rust Features**
   - Async/await from the ground up
   - Type-safe internal APIs
   - No unsafe code in outer layers
   - Idiomatic Rust patterns

5. **Extensibility**
   - Plugin system for custom functions
   - Custom collations
   - Virtual tables support
   - User-defined types

### Technical Strategy

**Phase Approach:**
1. Analyze SQLite architecture and file format
2. Implement file format reader/writer
3. Build core SQL engine with parser and planner
4. Implement execution engine
5. Optimize for performance
6. Create user-facing wrappers

**Why This Works:**
- Each phase is independently testable
- Can verify correctness against SQLite
- Performance optimization can be deferred
- Clear milestones for development

### Benchmarking Strategy

- Use SQLite test suite as reference
- Benchmark against rusqlite, SQLx, Diesel
- Focus on common OLTP operations
- Measure memory usage and startup time
- Profile with perf, flamegraph, etc.

## Key Insights

1. **Market Gap:** No pure Rust, SQLite-compatible SQL engine exists
2. **Performance Opportunity:** Custom Rust implementation could optimize for modern hardware
3. **Portability:** Pure Rust enables WASM and embedded targets
4. **Integration:** Could eventually replace C SQLite in existing bindings
5. **Educational Value:** Understanding SQLite internals is valuable

## Recommendations

1. Start with file format analysis - this is prerequisite for all future work
2. Use libSQL as reference for modern SQLite extensions
3. Benchmark frequently against C SQLite
4. Consider incremental compatibility - don't try to implement all features immediately
5. Focus on correctness first, performance second
