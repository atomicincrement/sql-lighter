# SQLite File Format Analysis

## Overview

SQLite stores all data in a single file using a carefully structured binary format. Understanding this format is critical for implementing a file reader/writer.

## File Structure

### File Header (First 100 bytes)

```
Offset  Size  Field
------  ----  -----
0       16    Magic string "SQLite format 3\x00"
16      2     Page size (bytes per page)
18      1     File format write version (1 for legacy, 3 for WAL, 4 for latest)
19      1     File format read version  
20      1     Bytes reserved per page end (usually 0)
21      1     Maximum embedded payload fraction (usually 64)
22      1     Minimum embedded payload fraction (usually 32)
23      1     Leaf page payload fraction (usually 32)
24      4     File change counter
28      4     Database size (in pages)
32      4     Freelist trunk page (0 if no freelist)
36      4     Total freelist pages
40      4     Schema cookie
44      4     Schema format number (4 for modern SQLite)
48      4     Default page cache size
52      4     Largest root B-tree page number
56      4     Database text encoding (1=UTF-8, 2=UTF-16LE, 3=UTF-16BE)
60      4     User version
64      4     Incremental vacuum mode
68      4     Application ID
72      20    Reserved for expansion (all zeros)
92      4     Version valid for number
96      4     SQLite version number
```

### Data Types

**Varint Encoding** - Variable-length integer encoding (1-9 bytes):
- Values 0-127: Single byte
- Values 128-16383: Two bytes  
- Values 16384-2097151: Three bytes
- Continues up to 9 bytes for full 64-bit integers

**Key Concepts:**
- Every database is divided into fixed-size pages
- Default page size is 4096 bytes
- Pages can be: B-tree internal nodes, B-tree leaves, overflow pages, etc.

## Page Structure

### B-tree Page Header (8 bytes)

```
Offset  Size  Field
------  ----  -----
0       1     Page type:
              - 0x02 = Index interior page
              - 0x05 = Table interior page
              - 0x0A = Index leaf page
              - 0x0D = Table leaf page
1       2     First freeblock offset (0 if no freelist)
3       2     Number of cells on page
5       2     Start of cell content area
7       1     Fragmented free bytes
8       4     Right-most pointer (interior pages only)
```

### B-tree Cell Structure

**Interior cells (pointer to child):**
```
Offset  Size  Field
------  ----  -----
0       4     Child page pointer
4       var   Key (index) or rowid (table)
```

**Leaf cells:**
```
Offset  Size  Field
------  ----  -----
0       var   Payload size (varint)
var     var   Rowid (table leaf cells only) (varint)
var     var   Payload data
```

## Record Format

Each cell payload contains a record with the following structure:

```
Offset  Size  Field
------  ----  -----
0       var   Header length (varint)
var     var   Column type codes (1 byte each, or varint if > 127 columns)
...     var   Column data (variable size based on type code)
```

### Column Type Codes

```
Value   Type            Storage
-----   ----            -------
0       NULL            (0 bytes)
1       INTEGER         8 bytes
2       REAL            8 bytes
3       TEXT            n bytes (UTF-8)
4       BLOB            n bytes
5-6     RESERVED
>= 7    TEXT/BLOB       Actual size derived from code
```

For codes >= 7:
- If even: (code-12)/2 bytes of BLOB data
- If odd: (code-13)/2 bytes of UTF-8 TEXT

## Index Structure

Indexes use the same B-tree structure but:
- Interior and leaf pages point to table rows via rowid
- Index keys are in indexed column(s)
- Uniqueness is enforced by SQLite query planner

## Schema Storage

SQLite stores all table and index definitions in the `sqlite_master` table:

```sql
CREATE TABLE sqlite_master(
  type TEXT,           -- 'table', 'index', 'view', 'trigger'
  name TEXT,           -- Object name
  tbl_name TEXT,       -- Associated table name
  rootpage INTEGER,    -- B-tree root page number
  sql TEXT             -- SQL CREATE statement
);
```

## Implementation Data Structures

### Rust Structures Needed

```rust
pub struct FileHeader {
    pub magic: [u8; 16],
    pub page_size: u16,
    pub write_version: u8,
    pub read_version: u8,
    pub reserved_per_page: u8,
    pub max_payload_fraction: u8,
    pub min_payload_fraction: u8,
    pub leaf_payload_fraction: u8,
    pub change_counter: u32,
    pub page_count: u32,
    pub freelist_trunk: u32,
    pub freelist_pages: u32,
    pub schema_cookie: u32,
    pub schema_format: u32,
    pub cache_size: u32,
    pub largest_root: u32,
    pub text_encoding: TextEncoding,
    pub user_version: u32,
    pub incremental_vacuum: u32,
    pub app_id: u32,
    pub version_valid: u32,
    pub version_number: u32,
}

pub enum TextEncoding {
    Utf8 = 1,
    Utf16LE = 2,
    Utf16BE = 3,
}

pub struct BTreePageHeader {
    pub page_type: PageType,
    pub first_freeblock: u16,
    pub cell_count: u16,
    pub cell_start: u16,
    pub fragmented_free: u8,
    pub right_pointer: Option<u32>, // Interior pages only
}

pub enum PageType {
    IndexInterior = 0x02,
    TableInterior = 0x05,
    IndexLeaf = 0x0A,
    TableLeaf = 0x0D,
}

pub struct BTreeCell {
    pub key: u64,           // Rowid for table, key for index
    pub payload: Vec<u8>,
    pub child_pointer: Option<u32>, // Interior cells only
}

pub struct Record {
    pub columns: Vec<RecordValue>,
}

pub enum RecordValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

pub struct Page {
    pub header: BTreePageHeader,
    pub cells: Vec<BTreeCell>,
    pub overflow_pages: Vec<u32>,
}
```

## Read/Write Operations

### Reading a Page

1. Seek to: `(page_number - 1) * page_size`
2. Read and parse page header (8 bytes)
3. For each cell pointer in page:
   - Seek to cell offset
   - Read cell data based on page type
4. Handle overflow pages if payload extends beyond page

### Writing a Page

1. Serialize page header
2. Serialize all cells
3. Update free space calculations
4. Write to file at: `(page_number - 1) * page_size`
5. Update file header if page count changed

## Performance Considerations

1. **Page Caching** - Cache frequently accessed pages in memory
2. **Buffer Pool** - Maintain a pool of pages to avoid repeated allocations
3. **Lazy Loading** - Don't load cell data until needed
4. **Block I/O** - Read/write full pages at once
5. **Memory Mapping** - Consider mmap for large files

## Compatibility Notes

- SQLite has been stable at version 3 for 20+ years
- File format hasn't changed significantly since 2004
- Need to support: regular format, WAL (Write-Ahead Log), RTree indexes
- Version number in header allows future format changes

## Key Challenges

1. **Varint Encoding** - Must correctly decode variable-length integers
2. **Overflow Handling** - Payloads can span multiple pages
3. **Freelist Management** - Tracking free pages for reuse
4. **Concurrency** - Multiple readers can access simultaneously
5. **Recovery** - Handle incomplete writes gracefully

## References

- SQLite documentation: https://www.sqlite.org/fileformat.html
- SQLite source: `format.c`, `btree.c`, `pager.c`
