# SQLite Write-Ahead Log (WAL) and Lock Mechanisms

## Executive Summary

SQLite's Write-Ahead Log (WAL) mode represents a paradigm shift from traditional journal-based recovery. Instead of writing changes directly to the database file and maintaining a rollback journal, WAL mode writes all changes to a separate log file first. This enables better concurrency, faster writes, and improved crash recovery. Lock management coordinates concurrent access between readers and writers, requiring careful state machine implementation and shared memory synchronization.

---

## Part 1: WAL Architecture

### 1.1 Core Concepts

**Traditional SQLite (Journal Mode):**
1. Read data into cache
2. Modify data in cache
3. Write modified pages to rollback journal
4. Fsync journal to disk
5. Write modified pages to main database
6. Fsync database to disk
7. Delete rollback journal

**WAL Mode:**
1. Read data into cache
2. Modify data in cache
3. Write modified pages to WAL file
4. Fsync WAL to disk (commit point - crash-safe here)
5. Update WAL index metadata
6. When checkpoint occurs: copy WAL pages back to main database

**Advantages of WAL:**
- Separates writes (sequential to WAL) from reads (from main DB)
- Multiple concurrent readers while writes occur
- Faster commits (only WAL write, not main DB rewrite)
- Natural async/await support (WAL writes don't block readers)
- Better performance with SSDs (sequential writes to WAL)
- Efficient for small, frequent transactions

**Disadvantages:**
- Multiple files to manage (-wal, -shm, -journal)
- Network filesystems have synchronization issues
- Requires shared memory support
- Slightly slower for bulk read-heavy workloads
- Checkpoint process can cause write stalls

### 1.2 WAL File Format

**File Structure:**

The WAL file contains a header followed by one or more "frames" (each frame represents a modified page):

```
WAL File Layout:
┌─────────────────────────────────────┐
│  WAL Header (32 bytes)              │
├─────────────────────────────────────┤
│  Frame 1 Header (24 bytes)          │
│  Frame 1 Page Data (variable)       │
├─────────────────────────────────────┤
│  Frame 2 Header (24 bytes)          │
│  Frame 2 Page Data (variable)       │
├─────────────────────────────────────┤
│  ...                                │
├─────────────────────────────────────┤
│  Checkpoint Marker (optional)       │
└─────────────────────────────────────┘
```

**WAL Header (first 32 bytes of WAL file):**

```
Offset  Size  Field                           Description
------  ----  -----                           -----------
0       4     Magic number                    0x377F0682 (identifies WAL file)
4       4     Format version                  Typically 3007000
8       4     Database page size              Must match main database
12      4     Checkpoint sequence             Incremented after checkpoint
16      4     Salt1                           Random salt for identifying versions
20      4     Salt2                           Random salt for identifying versions
24      4     Checksum1                       CRC32 of bytes 0-23
28      4     Checksum2                       CRC32 of bytes 24-27 + repeated
```

**Frame Header (24 bytes per frame):**

```
Offset  Size  Field                           Description
------  ----  -----                           -----------
0       4     Page number                     Which page in database this frame modifies
4       4     Frame commit size               Size of this frame (page data + trailer)
8       4     Database size after this frame  In pages (for checkpoint recovery)
12      4     Salt1 from WAL header          Must match to verify frame is valid
16      4     Salt2 from WAL header          Must match to verify frame is valid
20      4     Checksum1                       CRC32 of frame header bytes 0-19
24      4     Checksum2                       Covers frame header + page data
```

**Frame Structure:**

Each frame contains:
```
Frame = [24-byte header] + [page_size bytes of page data] + [optional trailer]
```

Frame trailer (if present, 4 bytes):
- Contains commit marker to indicate frame is committed
- All frames after this are committed in same transaction

### 1.3 Transactions and Commit Semantics

**Transaction Boundaries:**

1. **Begin Transaction:**
   - Call `PRAGMA journal_mode=WAL` to enable WAL mode
   - SQLite creates main database file, WAL file, and SHM (shared memory) file

2. **During Transaction:**
   - Modified pages accumulated in memory
   - No writes to disk yet

3. **Commit:**
   - All modified pages written as frames to WAL file
   - WAL file fsync'd to ensure durability
   - All-or-nothing property: either all frames or none

4. **Rollback:**
   - Modified pages discarded
   - WAL remains unchanged (can contain multiple uncommitted frames)

**Multi-frame Transactions:**

A single transaction can produce multiple frames:
```
Frame 1: Modifies pages [5, 12] (uncommitted)
Frame 2: Modifies page 8         (uncommitted)
Frame 3: Modifies page 5 again   (commit marker) ← COMMITTED
```

Only Frame 3 has the commit marker. Frames 1-2 are part of same transaction.

### 1.4 Checkpoints: Durability and Cleanup

Checkpoints are the mechanism to move WAL contents back to main database and truncate WAL file.

**Why Checkpoints Are Needed:**

- WAL file grows indefinitely without checkpoints
- Readers need both main DB and WAL for complete view
- Eventual need to sync changes back to main database

**Checkpoint Modes:**

**PASSIVE Checkpoint:**
```
- Doesn't block readers or writers
- Moves WAL pages to main DB while respecting active readers
- Leaves frames for active readers in WAL
- Most concurrent-friendly
- May leave WAL file partially full

Implementation:
1. Begin checkpoint
2. For each committed frame:
   - Skip if page is in use by reader
   - Copy page from WAL to main database
   - Mark frame as checkpointed
3. End checkpoint (continue monitoring readers)
```

**RESTART Checkpoint:**
```
- Waits for active readers to finish
- Then moves all WAL pages to main database
- Resets checkpoint sequence
- More complete than PASSIVE
- Briefly blocks readers

Implementation:
1. Begin checkpoint
2. Wait for all readers to finish
3. For each committed frame:
   - Copy page from WAL to main database
4. Reset SHM and WAL files
5. End checkpoint
```

**RESET Checkpoint:**
```
- Like RESTART but also verifies pages
- Includes integrity checking
- Slowest but most thorough
- Used after recovery from corruption

Implementation:
1. Same as RESTART
2. Additional: Verify checksums of copied pages
3. Reset database size if necessary
```

**TRUNCATE Checkpoint:**
```
- Most aggressive
- Blocks everything until complete
- Truncates WAL file to header
- Used when WAL file is too large
- Can cause stalls in high-concurrency scenarios

Implementation:
1. Acquire exclusive lock (blocks all)
2. Copy all WAL pages to main database
3. Truncate WAL file
4. Reset all SHM metadata
5. Release exclusive lock
```

**Automatic Checkpointing:**

SQLite automatically triggers checkpoints when:
- WAL file reaches configured size limit (default 4MB)
- `PRAGMA wal_autocheckpoint` timeout
- Database connection closes (RESTART checkpoint)
- App calls `PRAGMA wal_checkpoint`

### 1.5 Recovery from Crashes

**Crash Scenarios:**

**Scenario 1: Crash before WAL fsync**
```
State: Pages in OS cache but not yet on disk
Recovery: WAL file on disk incomplete/corrupt
Action: Ignore invalid frames (checksums don't match)
Result: Database unchanged, partial WAL frames discarded
```

**Scenario 2: Crash after WAL fsync but before checkpoint**
```
State: Complete committed frames in WAL, not yet copied to main DB
Recovery: WAL file on disk, main DB unchanged
Action: Replay valid frames from WAL on next connection
Result: Restore database to pre-crash state (ACID compliance)
```

**Scenario 3: Crash during checkpoint**
```
State: Some frames copied to main DB, others still in WAL
Recovery: Compare checksums and checkpoint sequence
Action: Continue checkpoint from last consistent point
Result: Complete checkpoint without data loss
```

**Recovery Algorithm:**

```
On Database Open:
1. Read main database header
   - Extract database page size
   - Extract checkpoint sequence #1

2. Check if WAL file exists
   - If not: database is in journal mode or clean
   - If yes: continue

3. Validate WAL header
   - Check magic number (0x377F0682)
   - Extract checkpoint sequence #2
   - Extract page size (must match main DB)
   - Extract salt values

4. Validate WAL frames
   - For each frame in WAL:
     - Check salt values match WAL header
     - Verify checksums
     - If any check fails: stop processing frames
     - Track valid frame count

5. Replay valid frames
   - If checkpoint sequence #2 > #1:
     - Some frames already moved to main DB
     - Skip frames already applied
   - Apply remaining frames to main database
   - Update main DB checkpoint sequence

6. Verify integrity
   - If crash detected mid-frame: handle gracefully
   - Verify database size matches expectations
   - Check page count consistency

7. Continue with new transactions
```

### 1.6 Concurrent Readers and Writers in WAL Mode

**Key Insight:** WAL enables readers and writers to coexist without blocking each other.

**Reader Strategy:**

```
Reader sees database as:
[Main Database] + [Relevant WAL Frames Since Checkpoint]

Multiple snapshots possible:
- Reader 1: reads from DB + WAL frames 1-5
- Reader 2: reads from DB + WAL frames 1-7 (newer transaction)
- Reader 3: reads from just DB (very conservative)

Each reader maintains:
- Start timestamp
- Reference to specific checkpoint sequence
- Cannot see frames written after it started
```

**Writer Strategy:**

```
Writer appends frames:
- Builds up in memory
- All-or-nothing commit: all frames written together
- Fsync once for entire transaction
- Updates SHM to signal readers

Multi-writer coordination:
- Only one writer at a time (via locks)
- Readers don't block writer
- Writer blocks next writer until commit
```

**Timeline Example:**

```
Time  Reader 1         Writer          Reader 2
────  ────────────────────────────────────────────
T0    BEGIN READ       BEGIN WRITE
T1    Read page 5      Modify page 5
T2    Read page 8      Modify page 8
T3                     COMMIT (frames 
                        in WAL)
T4    Read page 12                      BEGIN READ
T5    See data as of T0               See data as of T5
T6    (can still read old              (can see new
       snapshot)                       frames from writer)
T7    COMMIT READ
```

---

## Part 2: Lock File Mechanism

### 2.1 SQLite Lock System Overview

**Three Types of Lock Files:**

1. **Main Database Lock (-wal file)**: While not technically a lock file, records active checkpoints and frames
2. **Shared Memory Lock (-shm file)**: Manages lock states and metadata
3. **Journal Lock (-journal file)**: Used in journal mode (not WAL)

**Lock States (in WAL mode):**

When WAL mode is enabled, SQLite uses 4 lock states on pages within the SHM file:

```
UNLOCKED (0)   → Reader/Writer acquired no lock
SHARED (1)     → Reader can proceed; multiple readers allowed
RESERVED (2)   → Writer has reserved right to write
PENDING (3)    → Writer waiting for readers to finish
EXCLUSIVE (4)  → Writer has exclusive access
```

**Lock State Transitions:**

```
                  ┌─────────────┐
                  │ UNLOCKED (0)│
                  └─────┬───────┘
                        │
         ┌──────────────┼──────────────┐
         │              │              │
         ▼              ▼              ▼
    [SHARED]       [RESERVED]     [PENDING]
    (Readers)      (Intent)       (Waiting)
         │              │              │
         └──────────────┼──────────────┘
                        │
                        ▼
                   [EXCLUSIVE]
                   (Only Writer)
```

### 2.2 Shared Memory File (SHM) Format

The shared memory file coordinates all locking state and critical metadata.

**SHM File Structure:**

```
SHM File (max 32MB):
┌─────────────────────────────────────┐
│  WAL Index Header (192 bytes)       │  Offset 0
├─────────────────────────────────────┤
│  Lock Bytes (4096 bytes total)      │  Offset 192
│    Lock bytes 1-4 (readers)         │  
│    Lock byte 5 (unused)             │  
├─────────────────────────────────────┤
│  Checkpoint Metadata (512 bytes)    │  Offset 4288
├─────────────────────────────────────┤
│  WAL Index (variable)               │  Offset 4800
│    Frame index entries              │  
└─────────────────────────────────────┘
```

**WAL Index Header (192 bytes):**

```
Offset  Size  Field                           Description
------  ----  -----                           -----------
0       4     Version                         3007000 for modern SQLite
4       4     Unused                          Reserved
8       4     Checksum1                       CRC32 of header
12      4     Checksum2                       CRC32 continuation
16      4     Last valid index                Number of valid frames
20      4     Pages in WAL                    Uncommitted frames
24      4     Frame size                      Typically 24 (header) + page_size
28      4     Byte reserved                   Usually 0
32      4     Max frame index                 Sanity check
36      4     Checkpoint sequence             From WAL header
40      4     Unused bytes                    Reserved for extension
44      4     Database size                   In pages
48      4     Unused bytes                    Reserved
52      4     Unused bytes                    Reserved
...     ...   (more reserved fields to 192)
```

**Lock Bytes (4096 total bytes, starting at offset 192):**

```
Position  Size  Lock Type           Purpose
────────  ────  ────────           ─────────
192       4     SHARED_LOCK_BYTES  Up to 4 locks for readers (one per reader session)
196       4     RESERVED_LOCK_BYTE One lock for writer reservation
200       4     PENDING_LOCK_BYTE  One lock indicating pending writer
204       4     EXCLUSIVE_LOCK_BYTE One lock for exclusive writer access

Each lock position can be:
- 0x00: UNLOCKED
- 0x01: LOCKED
```

**Checkpoint Metadata (512 bytes at offset 4288):**

```
Offset  Size  Field                           Description
------  ----  -----                           -----------
0       4     Last checkpoint sequence       From previous successful checkpoint
4       4     Checkpoint size                Pages copied in checkpoint
8       4     Checkpoint frames              Frames processed
...
```

**WAL Index (starting at offset 4800):**

```
Each WAL frame gets an index entry:

Entry per frame:
- Page number that frame modifies
- Offset in WAL file where frame begins
- Frame size including page data
- Transaction ID (to group frames)
- Is-committed flag

Index allows fast lookup:
- O(1) to find which frames modify a specific page
- Quick iteration through committed frames
```

### 2.3 Lock File Mechanism Details

**Lock Acquisition Protocol:**

**For Writers (BEGIN WRITE):**

```
1. Start in UNLOCKED state
2. Request SHARED lock (allowed if no other writers have EXCLUSIVE)
3. Once SHARED: hold until transaction commit
4. Before commit: request RESERVED lock (serializes writers)
5. Wait for all readers to exit SHARED locks
6. Attempt PENDING lock (signals readers to wrap up)
7. Hold PENDING until all readers finish
8. Upgrade to EXCLUSIVE lock
9. Perform final commit
10. Release EXCLUSIVE lock
11. Drop SHARED lock
```

**For Readers (BEGIN READ):**

```
1. Start in UNLOCKED state
2. Request SHARED lock (allowed unless writer has PENDING/EXCLUSIVE)
3. Hold SHARED lock during entire read transaction
4. Must acknowledge current WAL snapshot
5. Release SHARED lock when done
```

**Conflict Matrix:**

```
              SHARED   RESERVED PENDING EXCLUSIVE
SHARED          ✓         ✓       ✗        ✗
RESERVED        ✓         ✗       ✗        ✗
PENDING         ✗         ✗       ✗        ✗
EXCLUSIVE       ✗         ✗       ✗        ✗

✓ = Can coexist
✗ = Conflict (blocks)
```

### 2.4 Lock State Machine

**Detailed State Transitions:**

```
Reader Path:
UNLOCKED → SHARED (acquire read lock) → SHARED (hold) → UNLOCKED (release)

Writer Path:
UNLOCKED 
  → SHARED (acquire read lock, necessary to read WAL)
  → RESERVED (signals intent to write, blocks other writers)
  → PENDING (waits for active readers)
  → EXCLUSIVE (upgrades, all readers must release)
  → Writes to WAL
  → PENDING → RESERVED → SHARED → UNLOCKED (staged release)

Starvation Prevention:
- PENDING lock prevents new readers from starting
- Existing readers must finish
- Prevents writer starvation
```

### 2.5 Shared Memory Synchronization

**Memory Mapping:**

```
Rust Example:
use memmap2::MmapMut;
use std::fs::File;

// Create or open SHM file
let file = File::open("database.db-shm")?;
let mmap = unsafe { MmapMut::map_mut(&file)? };

// Access lock bytes
let lock_bytes = &mmap[192..196]; // SHARED locks
let reserved_byte = &mmap[196];   // RESERVED lock
let pending_byte = &mmap[197];    // PENDING lock
let exclusive_byte = &mmap[198];  // EXCLUSIVE lock
```

**Atomic Lock Operations:**

```
Typical operation uses compare-and-swap:
- Read current lock state (0 or 1)
- If expected value: set new value
- If not: retry or fail

Rust pattern:
use std::sync::atomic::{AtomicU8, Ordering};

// On memory-mapped region
let shared_lock = unsafe { 
    &*(mmap.as_ptr() as *const AtomicU8)
};

// Try to acquire
if shared_lock.compare_exchange(
    0, 1, Ordering::SeqCst, Ordering::SeqCst
).is_ok() {
    // Lock acquired
}
```

**Inter-process Synchronization:**

```
Challenge: Multiple processes may access same database
Solution: File system locking + memory-mapped barriers

1. File locking (OS-level):
   - Use flock() or lockf()
   - Prevents process crashes from deadlock

2. Memory-mapped barriers:
   - Atomic operations on shared memory
   - Visible to all processes with mapping

3. Recovery protocol:
   - Detect dead processes (lock held but process gone)
   - Clean up after process crash
   - Restart checkpoint if needed
```

---

## Part 3: Journal Mode vs WAL Mode Comparison

### 3.1 Mode Overview

**Journal Mode (Default):**

```
Update Sequence:
1. Read pages → cache
2. Modify pages → cache
3. Write pages → rollback journal file
4. Fsync journal
5. Write pages → main database
6. Fsync database
7. Delete journal

Lock: Exclusive lock held from start of write to end
```

**WAL Mode:**

```
Update Sequence:
1. Read pages → cache
2. Modify pages → cache
3. Write pages → WAL file
4. Fsync WAL
5. (Checkpoint eventually moves pages to main DB)

Lock: Shared lock during reads, exclusive only for commit
```

### 3.2 Performance Characteristics

**Journal Mode Pros:**
- Simpler implementation (no SHM file needed)
- Single file format (main database only)
- Better for bulk operations
- No checkpoint overhead during normal operation
- Good for read-only workloads

**Journal Mode Cons:**
- Exclusive locks block all readers during writes
- Must write modified pages twice (journal + database)
- Slower commits for frequent small transactions
- Higher disk I/O for write-heavy workloads
- Reader must wait for writer to complete

**WAL Mode Pros:**
- Multiple concurrent readers during writes
- Faster commits (write WAL once, not database twice)
- Better for high-concurrency scenarios
- Excellent SSD performance (sequential writes)
- Many small transactions faster
- Better crash recovery

**WAL Mode Cons:**
- More complex implementation
- Multiple files to manage (-wal, -shm)
- Network filesystem issues (NFS can lose locks)
- Shared memory requirement
- Checkpoint can cause stalls
- Slightly larger on-disk footprint initially

### 3.3 Concurrency Comparison

**Scenario 1: 1 Writer + 10 Readers**

Journal Mode:
```
Writer blocks all readers from starting
Readers must wait for writer
Lock held for full transaction duration
```

WAL Mode:
```
Writer appends to WAL
Readers continue reading from main DB + relevant WAL frames
Readers may see slightly stale data (but consistent snapshot)
Concurrent execution
```

**Scenario 2: 5 Sequential Writers + Readers**

Journal Mode:
```
Sequential: Writer 1 → Wait → Writer 2 → Wait → ...
Only one writer at a time
Readers blocked between each writer
```

WAL Mode:
```
Writers queue up (acquire RESERVED locks)
Execute more concurrently
Readers progress while writers wait for checkpoint
```

### 3.4 When to Use Each Mode

**Use Journal Mode When:**
- Single-threaded application
- Bulk operations (large imports)
- Simple embedded database with no concurrency
- Network filesystem (WAL doesn't work reliably on NFS)
- Must minimize file count

**Use WAL Mode When:**
- Multiple readers needed during writes
- High transaction frequency (many small commits)
- OLTP-style workload
- SSD storage
- Async/await concurrency model
- Performance-critical application

---

## Part 4: Implementation Considerations for Rust

### 4.1 Required Data Structures

```rust
// WAL Header representation
pub struct WalHeader {
    magic: u32,              // 0x377F0682
    format: u32,             // 3007000
    page_size: u32,
    checkpoint_seq: u32,
    salt1: u32,
    salt2: u32,
    checksum1: u32,
    checksum2: u32,
}

// Frame header representation
pub struct FrameHeader {
    page_num: u32,
    commit_size: u32,
    db_size: u32,
    salt1: u32,
    salt2: u32,
    checksum1: u32,
    checksum2: u32,
}

// SHM lock state
pub enum LockState {
    Unlocked = 0,
    Shared = 1,
    Reserved = 2,
    Pending = 3,
    Exclusive = 4,
}

// Connection context for lock management
pub struct WalContext {
    wal_file: File,
    shm_file: MmapMut,
    page_size: u32,
    frames: Vec<FrameHeader>,
    checkpoints: Vec<CheckpointInfo>,
}

// Transaction state
pub enum TxState {
    Idle,
    Reading {
        start_checkpoint: u32,
    },
    Writing {
        start_checkpoint: u32,
        frames: Vec<(u32, Vec<u8>)>, // (page_num, page_data)
    },
}
```

### 4.2 Lock File Handling in Rust

**Basic Lock Operations:**

```rust
use memmap2::MmapMut;
use std::fs::File;
use std::sync::atomic::{AtomicU8, Ordering};

pub struct LockManager {
    shm: MmapMut,
    lock_offset: usize,
}

impl LockManager {
    pub fn acquire_shared_lock(&mut self) -> Result<()> {
        let lock_byte = unsafe {
            &*(self.shm.as_ptr().add(self.lock_offset) as *const AtomicU8)
        };
        
        // Spin until we can acquire
        loop {
            match lock_byte.compare_exchange(
                0, 1,
                Ordering::SeqCst,
                Ordering::SeqCst
            ) {
                Ok(_) => return Ok(()),
                Err(_) => {
                    // Backoff and retry
                    std::thread::sleep(std::time::Duration::from_micros(1));
                }
            }
        }
    }

    pub fn release_shared_lock(&mut self) -> Result<()> {
        let lock_byte = unsafe {
            &*(self.shm.as_ptr().add(self.lock_offset) as *const AtomicU8)
        };
        
        lock_byte.store(0, Ordering::SeqCst);
        Ok(())
    }
}
```

### 4.3 Memory-Mapped File Usage

**SHM File Mapping:**

```rust
use memmap2::MmapMut;
use std::fs::OpenOptions;

pub fn open_shm_file(db_path: &Path) -> Result<MmapMut> {
    let shm_path = db_path.with_extension("db-shm");
    
    // Create if doesn't exist, preserve if exists
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(shm_path)?;
    
    // Ensure minimum size (at least SHM header)
    if file.metadata()?.len() < 4800 {
        file.set_len(4800)?;
    }
    
    // Memory map
    let mmap = unsafe { MmapMut::map_mut(&file)? };
    Ok(mmap)
}

// Initialize SHM header
pub fn init_shm_header(mmap: &mut MmapMut, page_size: u32) {
    // Write version
    mmap[0..4].copy_from_slice(&3007000u32.to_le_bytes());
    
    // Write reserved fields (zeros)
    for i in 4..192 {
        mmap[i] = 0;
    }
    
    // Initialize lock bytes
    for i in 192..196 {
        mmap[i] = 0; // All unlocked initially
    }
}
```

### 4.4 WAL Frame Reading/Writing

```rust
pub struct WalManager {
    wal_file: File,
    page_size: u32,
}

impl WalManager {
    pub fn write_frame(&mut self, page_num: u32, page_data: &[u8]) -> Result<()> {
        // Build frame header
        let frame_header = FrameHeader {
            page_num,
            commit_size: page_data.len() as u32 + 24,
            db_size: 0, // Will be set at commit
            salt1: self.salt1,
            salt2: self.salt2,
            checksum1: 0, // Will be calculated
            checksum2: 0, // Will be calculated
        };
        
        // Serialize frame header
        let mut frame = Vec::new();
        frame.extend_from_slice(&frame_header.page_num.to_be_bytes());
        frame.extend_from_slice(&frame_header.commit_size.to_be_bytes());
        // ... more fields
        
        // Add page data
        frame.extend_from_slice(page_data);
        
        // Calculate and update checksums
        let checksum1 = self.crc32(&frame[0..20]);
        let checksum2 = self.crc32(&frame[20..]);
        
        // Write to WAL
        self.wal_file.write_all(&frame)?;
        self.wal_file.flush()?;
        
        Ok(())
    }

    pub fn read_frames(&mut self) -> Result<Vec<(u32, Vec<u8>)>> {
        let mut frames = Vec::new();
        
        self.wal_file.seek(std::io::SeekFrom::Start(32))?; // Skip header
        
        loop {
            let mut frame_header = [0u8; 24];
            if self.wal_file.read_exact(&mut frame_header).is_err() {
                break; // EOF
            }
            
            // Parse header
            let page_num = u32::from_be_bytes([
                frame_header[0], frame_header[1], 
                frame_header[2], frame_header[3]
            ]);
            let commit_size = u32::from_be_bytes([
                frame_header[4], frame_header[5], 
                frame_header[6], frame_header[7]
            ]);
            
            // Verify salts and checksums
            // ...
            
            // Read page data
            let mut page_data = vec![0u8; self.page_size as usize];
            self.wal_file.read_exact(&mut page_data)?;
            
            frames.push((page_num, page_data));
        }
        
        Ok(frames)
    }
}
```

### 4.5 Checkpoint Implementation

```rust
pub enum CheckpointMode {
    Passive,
    Restart,
    Reset,
    Truncate,
}

impl WalManager {
    pub fn checkpoint(
        &mut self,
        mode: CheckpointMode,
        main_db: &mut DatabaseFile,
    ) -> Result<()> {
        match mode {
            CheckpointMode::Passive => {
                // Copy frames while respecting readers
                for (page_num, page_data) in self.read_frames()? {
                    // Check if page is locked by reader
                    if !self.is_page_locked(page_num) {
                        main_db.write_page(page_num, &page_data)?;
                    }
                }
            },
            CheckpointMode::Restart => {
                // Acquire lock, wait for readers
                self.acquire_exclusive_lock()?;
                for (page_num, page_data) in self.read_frames()? {
                    main_db.write_page(page_num, &page_data)?;
                }
                self.reset_wal()?;
                self.release_exclusive_lock()?;
            },
            // ... other modes
        }
        
        Ok(())
    }

    pub fn reset_wal(&mut self) -> Result<()> {
        // Truncate WAL file to just header
        self.wal_file.set_len(32)?;
        self.wal_file.seek(std::io::SeekFrom::Start(0))?;
        
        // Update header with new checkpoint sequence
        // ...
        
        Ok(())
    }
}
```

### 4.6 Error Handling

```rust
#[derive(Debug)]
pub enum WalError {
    InvalidMagic,
    InvalidChecksum,
    InvalidPageNumber,
    LockTimeout,
    CheckpointFailed,
    CorruptedFrame,
    IoError(std::io::Error),
}

impl From<std::io::Error> for WalError {
    fn from(err: std::io::Error) -> Self {
        WalError::IoError(err)
    }
}

pub type WalResult<T> = Result<T, WalError>;
```

### 4.7 Testing Strategy

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_frame_serialization() {
        // Test frame header read/write
    }

    #[test]
    fn test_lock_state_transitions() {
        // Test lock acquisition/release
    }

    #[test]
    fn test_checkpoint_passive() {
        // Test passive checkpoint respects readers
    }

    #[test]
    fn test_crash_recovery() {
        // Test recovery from incomplete checkpoint
    }

    #[test]
    fn test_concurrent_readers() {
        // Test multiple readers with single writer
    }

    #[test]
    fn test_wal_magic_validation() {
        // Test WAL header validation
    }
}
```

---

## Part 5: Key Algorithms

### 5.1 WAL Recovery Algorithm

```
Algorithm: RECOVER_FROM_WAL()

Input: Database file, WAL file, SHM file
Output: Recovered database

1. READ_MAIN_DB_HEADER()
   - Extract page_size
   - Extract checkpoint_seq ← DB_SEQ

2. IF WAL_FILE_EXISTS() THEN
   3. READ_WAL_HEADER()
      - Verify magic (0x377F0682)
      - Extract page_size (must match main DB)
      - Extract checkpoint_seq ← WAL_SEQ
      - Extract salt values
   
   4. IF page_size != main_db.page_size THEN
      - ABORT (page size mismatch)
   
   5. VALIDATE_WAL_FRAMES()
      valid_frames ← []
      FOR EACH frame IN WAL_FILE DO
         IF VERIFY_FRAME(frame) THEN
            APPEND frame TO valid_frames
         ELSE
            BREAK (stop at first invalid)
         END
      END
   
   6. IF WAL_SEQ > DB_SEQ THEN
      // Some frames not yet applied
      REPLAY_FRAMES(valid_frames, start_from=DB_SEQ)
   END
   
ELSE
   7. Database is clean or in journal mode
   
END
```

### 5.2 Lock Acquisition with Timeout

```
Algorithm: ACQUIRE_LOCK(lock_type, timeout_ms)

Input: lock_type (SHARED, RESERVED, etc), timeout
Output: LockHandle or timeout error

1. start_time ← NOW()
2. LOOP
   3. current_lock ← READ_LOCK_STATE()
   4. IF CAN_ACQUIRE(current_lock, lock_type) THEN
      5. TRY_ATOMIC_UPDATE(current, new_lock)
         6. IF SUCCESS THEN
            7. RETURN LockHandle
         8. ELSE // CAS failed, another process won
            9. CONTINUE to next iteration
         END
      END
   10. ELSE
      11. elapsed ← NOW() - start_time
      12. IF elapsed > timeout_ms THEN
         13. RETURN TIMEOUT_ERROR
      14. ELSE
         15. SLEEP(backoff_microseconds)
         16. backoff ← MIN(backoff * 1.5, 10000) // exponential backoff
      END
   END
END
```

### 5.3 Checkpoint Progress Tracking

```
Algorithm: CHECKPOINT_PROGRESS(mode)

Input: checkpoint_mode (PASSIVE, RESTART, etc)
Output: checkpoint_result

1. GET_CURRENT_WAL_SIZE() → wal_size
2. frames_to_process ← COUNT_VALID_FRAMES_IN_WAL()
3. progress ← 0

4. FOR EACH frame IN wal_frames DO
   5. page_num ← frame.page_number
   6. IF mode == PASSIVE THEN
      7. IF IS_PAGE_LOCKED(page_num) THEN
         8. SKIP frame
         9. CONTINUE
      END
   END
   
   10. READ_PAGE_FROM_WAL(page_num) → page_data
   11. WRITE_PAGE_TO_DB(page_num, page_data)
   12. UPDATE_CHECKPOINT_METADATA(page_num)
   13. progress ← progress + 1
   
   14. IF progress % 100 == 0 THEN
      15. UPDATE_PROGRESS_BAR(progress, frames_to_process)
   END
END

16. FINALIZE_CHECKPOINT()
    - Update checkpoint sequence
    - Truncate or reset WAL
    - Sync metadata
```

---

## Part 6: Crash Recovery Examples

### 6.1 Example 1: Crash During Transaction

```
Timeline:

T0: Begin transaction
T1: Write 5 pages to WAL
T2: Write page 1 to WAL (frame 6)
T3: CRASH - before commit marker
T4: Fsync hasn't occurred yet

Recovery:

1. Read main DB header → checkpoint_seq = 100
2. Read WAL header → checkpoint_seq = 100, salt1=X, salt2=Y
3. Try to read frames from WAL
4. Frame 1-5: valid (salts match, checksums good)
5. Frame 6: invalid (incomplete, checksum fails) - STOP
6. Action: Discard frames 6+ as uncommitted
7. Database reverts to state before transaction started
8. Connection can open normally
```

### 6.2 Example 2: Crash During Checkpoint

```
Timeline:

T0: Checkpoint begins, checkpoint_seq = 100
T1: 50 pages copied from WAL to main DB
T2: CRASH - before checkpoint_seq updated
T3: Checkpoint not finalized

Recovery:

1. Read main DB header → checkpoint_seq = 99
2. Read WAL header → checkpoint_seq = 100
3. Read SHM checkpoint metadata → processed = 50 pages
4. Frames 1-50: already in main DB
5. Frames 51-100: still in WAL
6. Action: Continue checkpoint from frame 51
7. Checksum verification shows frames 51-100 still valid
8. Apply remaining frames
9. Update checkpoint_seq to 100
```

---

## Implementation Priority

### Phase 1 (Core WAL): 
- WAL file format reading/writing
- Basic frame serialization
- WAL header validation

### Phase 2 (Locking):
- SHM file creation/mapping
- Lock state machine (SHARED/EXCLUSIVE)
- Atomic lock operations

### Phase 3 (Checkpoints):
- Passive checkpoint (safest to implement first)
- Frame replay during recovery
- Checkpoint sequence tracking

### Phase 4 (Concurrency):
- Multi-reader support
- Reader snapshot isolation
- Writer queuing

### Phase 5 (Advanced):
- RESTART/TRUNCATE checkpoint modes
- Automatic checkpoint triggering
- Performance optimization

---

## References & Standards

- SQLite WAL Design: https://www.sqlite.org/wal.html
- Shared Memory Protocol: https://www.sqlite.org/walformat.html
- Lock Protocol Details: https://www.sqlite.org/wal.html#locks
- Crash Recovery: https://www.sqlite.org/fileformat.html#rollback
