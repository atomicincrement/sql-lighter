# SQL Dialect & Syntax Analysis

## SQLite SQL Dialect

SQLite supports a subset of standard SQL with some extensions. This document describes the syntax we need to support.

## Core SQL Statements

### SELECT Statement

```
SELECT [ DISTINCT | ALL ]
  result_column [, result_column ]*
FROM table_or_subquery
[ WHERE expr ]
[ GROUP BY expr [, expr ]* ]
[ HAVING expr ]
[ ORDER BY expr [ ASC | DESC ] [, expr [ ASC | DESC ] ]* ]
[ LIMIT expr [ OFFSET expr ] ]
```

**Features:**
- Column selection with aliases
- Multiple tables (JOIN)
- WHERE filtering
- Aggregate functions (COUNT, SUM, AVG, MIN, MAX, GROUP_CONCAT)
- DISTINCT results
- ORDER BY with ASC/DESC
- LIMIT/OFFSET pagination
- Subqueries in FROM clause
- Common Table Expressions (WITH clause)

### INSERT Statement

```
INSERT INTO table_name [ ( column_name [, column_name ]* ) ]
VALUES ( expr [, expr ]* ) [, ( expr [, expr ]* ) ]*

INSERT INTO table_name [ ( column_name [, column_name ]* ) ]
SELECT ...

INSERT OR REPLACE | IGNORE INTO table_name ...
```

**Features:**
- Single and bulk insert
- Column specification
- SELECT as source
- Conflict resolution (OR REPLACE, OR IGNORE)

### UPDATE Statement

```
UPDATE table_name
SET column_name = expr [, column_name = expr ]*
[ WHERE expr ]
[ ORDER BY expr [ ASC | DESC ] ]
[ LIMIT expr [ OFFSET expr ] ]
```

**Features:**
- Multiple column updates
- WHERE filtering
- Subqueries in SET clause
- ORDER BY and LIMIT support

### DELETE Statement

```
DELETE FROM table_name
[ WHERE expr ]
[ ORDER BY expr ]
[ LIMIT expr ]
```

**Features:**
- Conditional deletion
- Ordering and limiting

### CREATE TABLE Statement

```
CREATE TABLE [ IF NOT EXISTS ] table_name (
  column_name type [ column_constraint ]*,
  [ table_constraint ]*
)

Column Constraints:
  PRIMARY KEY [ ASC | DESC ]
  UNIQUE
  NOT NULL
  DEFAULT expr
  CHECK ( expr )
  COLLATE collation_name
  REFERENCES foreign_table_name ( foreign_column_name )

Table Constraints:
  PRIMARY KEY ( column_name [, column_name ]* )
  UNIQUE ( column_name [, column_name ]* )
  CHECK ( expr )
  FOREIGN KEY ( column_name ) REFERENCES ...
```

**Features:**
- Column and table constraints
- Primary keys (single and composite)
- Foreign keys
- Default values
- Collation support
- IF NOT EXISTS

### CREATE INDEX Statement

```
CREATE [ UNIQUE ] INDEX [ IF NOT EXISTS ] index_name
ON table_name ( column_name [ ASC | DESC ] [, column_name [ ASC | DESC ] ]* )
[ WHERE expr ]
```

**Features:**
- Single and multi-column indexes
- Unique constraints via index
- Partial indexes (WHERE clause)
- ASC/DESC ordering

### ALTER TABLE Statement

```
ALTER TABLE table_name
  RENAME TO new_table_name
  RENAME COLUMN column_name TO new_column_name
  ADD COLUMN column_def
  DROP COLUMN column_name
```

**Features:**
- Rename tables
- Add/drop columns (limited)
- Rename columns

### DROP Statements

```
DROP TABLE [ IF EXISTS ] table_name
DROP INDEX [ IF EXISTS ] index_name
DROP VIEW [ IF EXISTS ] view_name
DROP TRIGGER [ IF EXISTS ] trigger_name
```

## Data Types

SQLite uses dynamic typing, but supports these type affinities:

```
INTEGER     - Signed integers
REAL        - Floating point
TEXT        - Text strings (UTF-8)
BLOB        - Binary data
NUMERIC     - Numbers with high precision
```

**Type Coercion:**
- Values can be stored as different types than declared
- Automatic conversion happens on comparison/use
- NULL is valid for any type

## Expressions

### Literals

```
integer_literal     123, -456, 0x1F
real_literal        1.23, 1.0e-3
string_literal      'text', "text"
blob_literal        X'48656C6C6F'
null_literal        NULL
boolean_literal     TRUE, FALSE
```

### Operators

**Arithmetic:**
- `+`, `-`, `*`, `/`, `%`

**Comparison:**
- `=`, `<>` (!=), `<`, `<=`, `>`, `>=`
- `IS`, `IS NOT`
- `LIKE`, `GLOB`
- `MATCH`, `REGEXP` (with extension)
- `BETWEEN`, `IN`

**Logical:**
- `AND`, `OR`, `NOT`

**Bitwise:**
- `&`, `|`, `<<`, `>>`

**String:**
- `||` (concatenation)

**Collation:**
- `expr COLLATE collation_name`

## Functions

### Aggregate Functions
```
COUNT(*)
COUNT(distinct expr)
SUM(expr)
AVG(expr)
MIN(expr)
MAX(expr)
GROUP_CONCAT(expr [, separator])
TOTAL(expr)
```

### Scalar Functions
```
ABS(x)
CAST(expr AS type)
COALESCE(x, y, ...)
GLOB(pattern, text)
IFNULL(x, y)
INSTR(haystack, needle)
LENGTH(str)
LIKE(pattern, text [, escape])
LOWER(str)
LTRIM(str)
MAX(x, y, ...)
MIN(x, y, ...)
NULLIF(x, y)
PRINTF(format, ...)
QUOTE(x)
RANDOM()
REPLACE(str, from, to)
ROUND(x [, digits])
RTRIM(str)
SUBSTR(str, start [, length])
TRIM(str)
TYPEOF(x)
UPPER(str)
```

### DateTime Functions
```
DATE(timestring [, modifier]*)
TIME(timestring [, modifier]*)
DATETIME(timestring [, modifier]*)
JULIANDAY(timestring [, modifier]*)
STRFTIME(format, timestring [, modifier]*)
```

## Query Modifiers

### Common Table Expressions (WITH)

```
WITH cte_name(col1, col2) AS (
  SELECT ...
)
SELECT * FROM cte_name
```

### Window Functions

```
SELECT col,
  SUM(value) OVER (PARTITION BY category ORDER BY date) as running_total
FROM table
```

**Supported:**
- ROW_NUMBER(), RANK(), DENSE_RANK()
- LAG(), LEAD()
- FIRST_VALUE(), LAST_VALUE()
- Aggregate functions as window functions

### CASE Expression

```
CASE
  WHEN condition THEN result
  [WHEN condition THEN result]*
  [ELSE result]
END

CASE expr
  WHEN value THEN result
  [WHEN value THEN result]*
  [ELSE result]
END
```

## JOIN Types

```
FROM table1
  [INNER] JOIN table2 ON condition
  LEFT [OUTER] JOIN table2 ON condition
  RIGHT [OUTER] JOIN table2 ON condition
  FULL [OUTER] JOIN table2 ON condition
  CROSS JOIN table2
  NATURAL JOIN table2
```

## Conflict Resolution

```
INSERT OR REPLACE
INSERT OR IGNORE
INSERT OR FAIL
INSERT OR ROLLBACK
INSERT OR ABORT

UPDATE OR ... 
DELETE OR ...
```

## Transaction Control

```
BEGIN [TRANSACTION] [DEFERRED | IMMEDIATE | EXCLUSIVE]
COMMIT [TRANSACTION]
ROLLBACK [TRANSACTION]
SAVEPOINT name
RELEASE SAVEPOINT name
ROLLBACK TO name
```

## Pragma Statements

```
PRAGMA key = value
PRAGMA function_list()
PRAGMA table_info(table_name)
PRAGMA index_info(index_name)
PRAGMA database_list()
```

## Parser Implementation Requirements

### Tokens

The lexer must recognize:
- Keywords (CREATE, SELECT, etc.)
- Identifiers (table/column names)
- String literals ('...')
- Numeric literals (123, 1.23)
- Operators (+, -, *, /, etc.)
- Comments (-- and /* */)
- Whitespace

### Parser Strategy

Use a **recursive descent parser** or **precedence climbing** for expressions:

1. **Lexer** - Tokenize input into tokens
2. **Parser** - Build Abstract Syntax Tree (AST)
3. **Validator** - Check semantic validity
4. **Optimizer** - Simplify and reorder operations

### Data Structures

```rust
pub enum Statement {
    Select(SelectStmt),
    Insert(InsertStmt),
    Update(UpdateStmt),
    Delete(DeleteStmt),
    CreateTable(CreateTableStmt),
    CreateIndex(CreateIndexStmt),
    DropTable(DropTableStmt),
    // ... others
}

pub struct SelectStmt {
    pub distinct: bool,
    pub columns: Vec<ResultColumn>,
    pub from: Option<FromClause>,
    pub where_expr: Option<Expr>,
    pub group_by: Option<Vec<Expr>>,
    pub having: Option<Expr>,
    pub order_by: Option<Vec<OrderBy>>,
    pub limit: Option<(Expr, Option<Expr>)>, // (count, offset)
}

pub enum Expr {
    Literal(Literal),
    Column(String, Option<String>), // (column, optional_table)
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Function {
        name: String,
        args: Vec<Expr>,
    },
    Case {
        operand: Option<Box<Expr>>,
        when_clauses: Vec<(Expr, Expr)>,
        else_expr: Option<Box<Expr>>,
    },
    Subquery(Box<SelectStmt>),
    Between {
        expr: Box<Expr>,
        low: Box<Expr>,
        high: Box<Expr>,
    },
    In {
        expr: Box<Expr>,
        values: Vec<Expr>,
    },
    Like {
        expr: Box<Expr>,
        pattern: Box<Expr>,
        escape: Option<Box<Expr>>,
    },
    IsNull(Box<Expr>),
    IsNotNull(Box<Expr>),
}

pub enum Literal {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
    Boolean(bool),
}
```

## Implementation Priority

**Phase 1 (MVP):**
- Basic SELECT with WHERE
- INSERT/UPDATE/DELETE
- CREATE TABLE with simple types
- WHERE with =, <, >, <=, >=, AND, OR
- Basic functions: COUNT, SUM, AVG, MIN, MAX

**Phase 2:**
- JOINs
- GROUP BY, HAVING
- ORDER BY
- Subqueries
- More functions

**Phase 3:**
- Window functions
- CTEs
- Complex expressions
- CASE statements

**Phase 4:**
- Views
- Triggers
- Foreign keys
- Advanced constraints

## References

- SQLite SQL documentation: https://www.sqlite.org/lang.html
- SQL standard: ISO/IEC 9075
- SQLite source: `parse.y`, `tokenize.c`
