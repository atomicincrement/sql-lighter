# Query Planner Architecture Analysis

## Overview

The query planner transforms a parsed SQL statement into an optimized execution plan. SQLite uses a cost-based optimizer that evaluates different execution strategies and selects the most efficient one.

## Query Planning Pipeline

```
Parsed Statement (AST)
        ↓
Name Resolution (validate columns, tables exist)
        ↓
Type Checking (ensure type compatibility)
        ↓
Logical Query Plan (normalized form)
        ↓
Statistics Analysis (estimate row counts)
        ↓
Join Order Optimization (find best join order)
        ↓
Physical Query Plan (with access paths)
        ↓
Code Generation (virtual machine instructions)
```

## Data Structures for Planning

### Query Plan Node

```rust
pub enum PlanNode {
    // Data Access
    SeqScan {
        table: String,
        filters: Vec<Expr>,
    },
    IndexScan {
        table: String,
        index: String,
        key_values: Vec<Expr>,
        filters: Vec<Expr>,
    },
    PrimaryKeyScan {
        table: String,
        key_values: Vec<Expr>,
    },
    
    // Set Operations
    Union {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        distinct: bool,
    },
    Intersect {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
    },
    Except {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
    },
    
    // Join Operations
    NestedLoopJoin {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Option<Expr>,
        join_type: JoinType,
    },
    HashJoin {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Expr,
        join_type: JoinType,
    },
    MergeJoin {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        condition: Expr,
        join_type: JoinType,
    },
    
    // Aggregation
    Aggregate {
        child: Box<PlanNode>,
        group_by: Vec<Expr>,
        aggregates: Vec<(String, AggregateFunc)>,
        having: Option<Expr>,
    },
    
    // Ordering
    Sort {
        child: Box<PlanNode>,
        order_by: Vec<(Expr, bool)>, // (expression, is_asc)
    },
    
    // Limiting
    Limit {
        child: Box<PlanNode>,
        count: usize,
        offset: usize,
    },
    
    // Projection
    Project {
        child: Box<PlanNode>,
        columns: Vec<(Expr, String)>, // (expression, alias)
    },
    
    // Filtering
    Filter {
        child: Box<PlanNode>,
        condition: Expr,
    },
}

pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

pub enum AggregateFunc {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    GroupConcat,
}
```

### Statistics

```rust
pub struct TableStatistics {
    pub row_count: usize,
    pub page_count: usize,
    pub column_stats: HashMap<String, ColumnStatistics>,
}

pub struct ColumnStatistics {
    pub distinct_values: usize,
    pub null_count: usize,
    pub min_value: Option<SqlValue>,
    pub max_value: Option<SqlValue>,
    pub cardinality: f64, // distinct values / total rows
}

pub struct IndexStatistics {
    pub index_name: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub pages: usize,
    pub row_count: usize,
}
```

## Query Optimization Strategies

### 1. Join Order Optimization

**Problem:** For N tables, there are N! possible join orders.

**Solution:**
- **Small N (≤ 8 tables):** Exhaustive search with pruning
- **Large N:** Heuristic approach
  - Estimate cost of each possible join order
  - Prune branches with higher cost than current best
  - Use dynamic programming

**Cost Model:**
```
cost = (input_rows) * (index_lookup_cost | scan_cost) + output_rows * output_cost
```

### 2. Predicate Pushdown

Move filters as close to data source as possible:

```
BEFORE:
  Project
    └─ Filter (WHERE status = 'active')
       └─ Join
          ├─ Scan users
          └─ Scan orders

AFTER:
  Project
    └─ Join
       ├─ Scan users
       └─ Filter (WHERE status = 'active')
          └─ Scan orders
```

### 3. Null Rejection

If a table's rows can be eliminated by a filter before joining:

```sql
SELECT * FROM users u
  LEFT JOIN orders o ON u.id = o.user_id
WHERE o.id IS NOT NULL

-- Optimize to:
SELECT * FROM users u
  INNER JOIN orders o ON u.id = o.user_id
```

### 4. Constant Folding

Evaluate expressions with only constants:

```
WHERE date_added > DATE('2024-01-01')
→ WHERE date_added > 1704067200
```

### 5. Boolean Simplification

```
WHERE x = 5 AND x = 5  →  WHERE x = 5
WHERE x > 5 AND x > 10  →  WHERE x > 10
WHERE TRUE AND y > 0  →  WHERE y > 0
WHERE FALSE OR y > 0  →  WHERE y > 0
```

### 6. Index Selection

For each table access:

```
Available indexes:
  - Primary key
  - Indexes on filtered columns
  - Partial indexes matching WHERE clause

Score each index:
  - Can be used? (column in WHERE or JOIN ON)
  - How many columns does it cover?
  - Is it partial and applicable?
  - Does it support ordering?

Select highest-scoring viable index
```

### 7. Join Type Selection

**Inner Join:**
- Nested Loop: Always works, O(n*m)
- Hash Join: Fast for large tables with equality condition
- Merge Join: Fast if inputs pre-sorted or indexes available
- Lookup Join: If inner table indexed on join key

**Outer Join:**
- Must preserve nulls for outer table
- Can be transformed to inner join if WHERE clause has IS NOT NULL

**Implementation Strategy:**
```rust
fn estimate_join_cost(
    left_rows: usize,
    right_rows: usize,
    join_type: JoinType,
    condition: &Expr,
) -> QueryCost {
    // Return cost for each join method
    // Planner picks the minimum
}
```

## Virtual Machine Bytecode

The planner generates bytecode instructions for the execution engine:

```rust
pub enum Opcode {
    // Data loading
    OpenRead { table_id: u32, cursor: u32 },
    OpenWrite { table_id: u32, cursor: u32 },
    Rewind { cursor: u32, goto_line: u32 },
    Next { cursor: u32, goto_line: u32 },
    Column { cursor: u32, column: u32, register: u32 },
    RowId { cursor: u32, register: u32 },
    
    // Data manipulation
    NewRow { table_id: u32, cursor: u32, register: u32 },
    InsertRow { cursor: u32, rowid_register: u32 },
    DeleteRow { cursor: u32 },
    
    // Aggregation
    AggStep { func: AggregateFunc, src: u32, dst: u32 },
    AggFinal { func: AggregateFunc, dst: u32 },
    
    // Control flow
    If { condition: Condition, goto_line: u32 },
    IfNot { condition: Condition, goto_line: u32 },
    Goto { line: u32 },
    
    // Expression evaluation
    Scalarfunc { func_id: u32, args: Vec<u32>, result: u32 },
    Add { left: u32, right: u32, result: u32 },
    Subtract { left: u32, right: u32, result: u32 },
    // ... other operators
    
    // Output
    ResultRow { start: u32, count: u32 },
    Close { cursor: u32 },
}
```

## Cost Model

```rust
pub struct QueryCost {
    pub io_cost: f64,           // Page reads
    pub cpu_cost: f64,          // Comparisons, function calls
    pub memory_cost: f64,       // Temporary storage
    
    pub total: f64,             // io_cost + cpu_cost * 10 + memory_cost
}

impl QueryCost {
    pub fn sequential_scan(pages: usize) -> Self {
        Self {
            io_cost: pages as f64,
            cpu_cost: 0.0,
            memory_cost: 0.0,
        }
    }
    
    pub fn index_lookup(index_depth: usize) -> Self {
        Self {
            io_cost: (index_depth + 1) as f64,
            cpu_cost: 10.0,
            memory_cost: 0.0,
        }
    }
    
    pub fn nested_loop_join(left_rows: usize, right_cost: QueryCost) -> Self {
        let mut cost = right_cost;
        cost.io_cost *= left_rows as f64;
        cost.cpu_cost += left_rows as f64 * 2.0;
        cost
    }
}
```

## Planner Implementation Steps

### Step 1: Logical Planning

Convert AST into logical query plan:

```rust
fn logical_plan(stmt: &SelectStmt, schema: &Schema) -> Result<PlanNode> {
    // 1. Validate all tables and columns exist
    // 2. Resolve column types
    // 3. Build logical plan tree
    // 4. Apply logical optimizations
}
```

### Step 2: Statistics Collection

Gather table and index statistics:

```rust
fn collect_statistics(table: &Table) -> TableStatistics {
    // Scan table to determine:
    // - Row count
    // - Page count
    // - Distinct values per column
    // - NULL count per column
    // Cache results for planner reuse
}
```

### Step 3: Physical Planning

Select access methods and join algorithms:

```rust
fn physical_plan(logical: PlanNode, stats: &Statistics) -> PlanNode {
    match logical {
        PlanNode::SeqScan { table, filters } => {
            // Can we use an index?
            if let Some(index) = find_useful_index(table, filters) {
                PlanNode::IndexScan { /* ... */ }
            } else {
                PlanNode::SeqScan { /* ... */ }
            }
        }
        PlanNode::NestedLoopJoin { left, right, condition, join_type } => {
            // Estimate costs for different join methods
            // Return cheapest
        }
        // ... handle other cases
    }
}
```

### Step 4: Code Generation

Generate virtual machine instructions:

```rust
fn generate_bytecode(plan: &PlanNode) -> Vec<Opcode> {
    let mut bytecode = Vec::new();
    generate_bytecode_recursive(plan, &mut bytecode);
    bytecode
}
```

## Optimization Passes

1. **Predicate Pushdown** - Move filters close to data source
2. **Constant Folding** - Evaluate constant expressions  
3. **Common Subexpression Elimination** - Avoid recomputing same expr
4. **Dead Code Elimination** - Remove unused columns
5. **Join Reordering** - Find optimal join order
6. **Index Selection** - Choose best index for each table
7. **Materialization** - Decide which results to cache

## Implementation Data Structures Summary

Key Rust types needed:

```rust
// Query plan
pub enum PlanNode { /* ... */ }

// Statistics
pub struct TableStatistics { /* ... */ }
pub struct ColumnStatistics { /* ... */ }

// Cost estimation
pub struct QueryCost { /* ... */ }

// Optimization context
pub struct PlannerContext {
    pub schema: Schema,
    pub statistics: Statistics,
    pub config: OptimizerConfig,
}

// Virtual machine
pub enum Opcode { /* ... */ }

// Planner state
pub struct Planner {
    pub context: PlannerContext,
}
```

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| SeqScan | O(n) | Full table scan |
| IndexLookup | O(log n) | B-tree lookup |
| JoinNL | O(n*m) | Nested loop, worst case |
| JoinHash | O(n+m) | Hash join, best case |
| Sort | O(n log n) | If not pre-sorted |
| GroupBy | O(n log n) | With sorting |

## References

- SQLite source: `analyze.c`, `select.c`, `whereexpr.c`
- Query optimization papers and textbooks
- Cost model information: https://www.sqlite.org/queryplanner.html
