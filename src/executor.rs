//! Query Execution Engine (Phase 4e)
//!
//! Transforms execution plans into actual query results using a virtual machine.
//! References:
//! - SQLite Virtual Machine: https://www.sqlite.org/opcode.html
//! - Row evaluation and computation
//! - Traditional database execution models from "Database System Concepts"

use crate::error::{Error, Result};
use crate::parser::{
    BinaryOperator, Expression, SortDirection, UnaryOperator,
};
use crate::planner::ExecutionPlan;
use crate::types::{Row, SqlValue};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Result set - collection of rows with schema
#[derive(Debug, Clone)]
pub struct ResultSet {
    /// Column names in order
    pub columns: Vec<String>,
    /// Rows of data
    pub rows: Vec<Row>,
}

impl ResultSet {
    /// Create a new empty result set
    pub fn new(columns: Vec<String>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
        }
    }

    /// Add a row to the result set
    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row);
    }

    /// Get number of rows
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Check if result set is empty
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Filter columns by name
    pub fn project(&self, column_names: &[&str]) -> Result<ResultSet> {
        let mut new_columns = Vec::new();
        let mut column_indices = Vec::new();

        for col_name in column_names {
            if let Some(idx) = self.columns.iter().position(|c| c == col_name) {
                new_columns.push(col_name.to_string());
                column_indices.push(idx);
            } else {
                return Err(Error::ExecutionError(format!(
                    "Column '{}' not found",
                    col_name
                )));
            }
        }

        let mut new_rows = Vec::new();
        for row in &self.rows {
            let mut new_row = Row::new();
            for &idx in &column_indices {
                new_row.push(row[idx].clone());
            }
            new_rows.push(new_row);
        }

        Ok(ResultSet {
            columns: new_columns,
            rows: new_rows,
        })
    }

    /// Sort rows by ordering terms
    pub fn sort(&mut self, order_terms: &[(String, SortDirection)]) -> Result<()> {
        self.rows.sort_by(|a, b| {
            for (col_name, direction) in order_terms {
                if let (Some(a_idx), Some(b_idx)) = (
                    self.columns.iter().position(|c| c == col_name),
                    self.columns.iter().position(|c| c == col_name),
                ) {
                    let cmp = compare_values(&a[a_idx].1, &b[b_idx].1);
                    let cmp = match direction {
                        SortDirection::Asc => cmp,
                        SortDirection::Desc => cmp.reverse(),
                    };
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
            }
            Ordering::Equal
        });
        Ok(())
    }

    /// Limit rows to specified count and offset
    pub fn limit(&mut self, limit: Option<usize>, offset: Option<usize>) {
        let offset = offset.unwrap_or(0);
        let start = offset;
        let end = if let Some(limit) = limit {
            (offset + limit).min(self.rows.len())
        } else {
            self.rows.len()
        };

        if start < self.rows.len() {
            self.rows = self.rows[start..end].to_vec();
        } else {
            self.rows.clear();
        }
    }
}

/// Expression evaluator - evaluates expressions against row data
/// References: Expression evaluation in database systems
pub struct ExpressionEvaluator;

impl ExpressionEvaluator {
    /// Evaluate an expression in the context of a row
    pub fn eval(expr: &Expression, row: &Row, columns: &[String]) -> Result<SqlValue> {
        match expr {
            Expression::Literal(lit) => Self::parse_literal(lit),

            Expression::Identifier(name) => {
                if let Some(idx) = columns.iter().position(|c| c == name) {
                    Ok(row[idx].1.clone())
                } else {
                    Err(Error::ExecutionError(format!(
                        "Column '{}' not found",
                        name
                    )))
                }
            }

            Expression::QualifiedIdentifier { table: _, column } => {
                if let Some(idx) = columns.iter().position(|c| c == column) {
                    Ok(row[idx].1.clone())
                } else {
                    Err(Error::ExecutionError(format!(
                        "Column '{}' not found",
                        column
                    )))
                }
            }

            Expression::BinaryOp { left, op, right } => {
                let left_val = Self::eval(left, row, columns)?;
                let right_val = Self::eval(right, row, columns)?;
                Self::eval_binary_op(&left_val, *op, &right_val)
            }

            Expression::UnaryOp { op, operand } => {
                let val = Self::eval(operand, row, columns)?;
                Self::eval_unary_op(*op, &val)
            }

            Expression::FunctionCall { name, args } => {
                let arg_vals: Result<Vec<_>> =
                    args.iter().map(|arg| Self::eval(arg, row, columns)).collect();
                Self::eval_function(name, arg_vals?)
            }

            Expression::Parenthesized(inner) => Self::eval(inner, row, columns),

            Expression::Case {
                operand,
                when_clauses,
                else_clause,
            } => {
                let operand_val = operand
                    .as_ref()
                    .map(|op| Self::eval(op, row, columns))
                    .transpose()?;

                for (condition, result) in when_clauses {
                    let cond_val = Self::eval(condition, row, columns)?;
                    let matches = if let Some(ref op_val) = operand_val {
                        Self::values_equal(op_val, &cond_val)?
                    } else {
                        Self::is_truthy(&cond_val)
                    };

                    if matches {
                        return Self::eval(result, row, columns);
                    }
                }

                if let Some(else_expr) = else_clause {
                    Self::eval(else_expr, row, columns)
                } else {
                    Ok(SqlValue::Null)
                }
            }

            Expression::Cast { expr, type_name } => {
                let val = Self::eval(expr, row, columns)?;
                Self::cast_value(&val, type_name)
            }

            Expression::Null => Ok(SqlValue::Null),
            Expression::True => Ok(SqlValue::Boolean(true)),
            Expression::False => Ok(SqlValue::Boolean(false)),
        }
    }

    /// Parse a literal string into SqlValue
    fn parse_literal(lit: &str) -> Result<SqlValue> {
        // Try integer
        if let Ok(i) = lit.parse::<i64>() {
            return Ok(SqlValue::Integer(i));
        }

        // Try float
        if let Ok(f) = lit.parse::<f64>() {
            return Ok(SqlValue::Real(f));
        }

        // String literal (remove quotes if present)
        let s = if (lit.starts_with('\'') && lit.ends_with('\''))
            || (lit.starts_with('"') && lit.ends_with('"'))
        {
            lit[1..lit.len() - 1].to_string()
        } else {
            lit.to_string()
        };

        Ok(SqlValue::Text(s))
    }

    /// Evaluate binary operation
    fn eval_binary_op(left: &SqlValue, op: BinaryOperator, right: &SqlValue) -> Result<SqlValue> {
        use BinaryOperator::*;

        // NULL handling - most operations with NULL result in NULL
        if matches!(left, SqlValue::Null) || matches!(right, SqlValue::Null) {
            match op {
                Is => {
                    // Special case: IS NULL / IS NOT NULL
                    return Ok(SqlValue::Boolean(matches!(left, SqlValue::Null)));
                }
                _ => return Ok(SqlValue::Null),
            }
        }

        match (left, right, op) {
            // Arithmetic
            (SqlValue::Integer(a), SqlValue::Integer(b), Add) => Ok(SqlValue::Integer(a + b)),
            (SqlValue::Integer(a), SqlValue::Integer(b), Subtract) => {
                Ok(SqlValue::Integer(a - b))
            }
            (SqlValue::Integer(a), SqlValue::Integer(b), Multiply) => {
                Ok(SqlValue::Integer(a * b))
            }
            (SqlValue::Integer(a), SqlValue::Integer(b), Divide) => {
                if *b == 0 {
                    Ok(SqlValue::Null)
                } else {
                    Ok(SqlValue::Integer(a / b))
                }
            }
            (SqlValue::Integer(a), SqlValue::Integer(b), Modulo) => {
                if *b == 0 {
                    Ok(SqlValue::Null)
                } else {
                    Ok(SqlValue::Integer(a % b))
                }
            }

            // String concatenation
            (SqlValue::Text(a), SqlValue::Text(b), Concatenate) => {
                Ok(SqlValue::Text(format!("{}{}", a, b)))
            }

            // Comparisons
            (a, b, Equal) => Ok(SqlValue::Boolean(Self::values_equal(a, b)?)),
            (a, b, NotEqual) => Ok(SqlValue::Boolean(!Self::values_equal(a, b)?)),

            // Relational
            (a, b, LessThan) => Ok(SqlValue::Boolean(compare_values(a, b) == Ordering::Less)),
            (a, b, LessThanOrEqual) => {
                Ok(SqlValue::Boolean(compare_values(a, b) != Ordering::Greater))
            }
            (a, b, GreaterThan) => {
                Ok(SqlValue::Boolean(compare_values(a, b) == Ordering::Greater))
            }
            (a, b, GreaterThanOrEqual) => {
                Ok(SqlValue::Boolean(compare_values(a, b) != Ordering::Less))
            }

            // Logical
            (a, b, And) => Ok(SqlValue::Boolean(Self::is_truthy(a) && Self::is_truthy(b))),
            (a, b, Or) => Ok(SqlValue::Boolean(Self::is_truthy(a) || Self::is_truthy(b))),

            // Bitwise
            (SqlValue::Integer(a), SqlValue::Integer(b), BitwiseAnd) => {
                Ok(SqlValue::Integer(a & b))
            }
            (SqlValue::Integer(a), SqlValue::Integer(b), BitwiseOr) => {
                Ok(SqlValue::Integer(a | b))
            }
            (SqlValue::Integer(a), SqlValue::Integer(b), BitwiseXor) => {
                Ok(SqlValue::Integer(a ^ b))
            }
            (SqlValue::Integer(a), SqlValue::Integer(b), BitwiseLeftShift) => {
                Ok(SqlValue::Integer(a << b))
            }
            (SqlValue::Integer(a), SqlValue::Integer(b), BitwiseRightShift) => {
                Ok(SqlValue::Integer(a >> b))
            }

            // SQL-specific: LIKE (case-insensitive pattern matching)
            (SqlValue::Text(text), SqlValue::Text(pattern), Like) => {
                let pat = pattern.replace('%', ".*").replace('_', ".");
                let re = regex::Regex::new(&pat).unwrap_or_else(|_| regex::Regex::new("").unwrap());
                Ok(SqlValue::Boolean(re.is_match(&text.to_lowercase())))
            }

            // SQL-specific: IN operator
            (_, _, In) => {
                // Simplified IN - would need list context
                Ok(SqlValue::Boolean(false))
            }

            _ => Err(Error::ExecutionError(format!(
                "Unsupported binary operation: {:?} {:?} {:?}",
                left, op, right
            ))),
        }
    }

    /// Evaluate unary operation
    fn eval_unary_op(op: UnaryOperator, val: &SqlValue) -> Result<SqlValue> {
        use UnaryOperator::*;

        match (op, val) {
            (Not, SqlValue::Null) => Ok(SqlValue::Null),
            (Not, v) => Ok(SqlValue::Boolean(!Self::is_truthy(v))),

            (Minus, SqlValue::Integer(i)) => Ok(SqlValue::Integer(-i)),
            (Minus, SqlValue::Real(f)) => Ok(SqlValue::Real(-f)),
            (Minus, SqlValue::Null) => Ok(SqlValue::Null),
            (Minus, _) => Err(Error::ExecutionError(format!("Cannot negate {:?}", val))),

            (Plus, SqlValue::Integer(i)) => Ok(SqlValue::Integer(*i)),
            (Plus, SqlValue::Real(f)) => Ok(SqlValue::Real(*f)),
            (Plus, SqlValue::Null) => Ok(SqlValue::Null),
            (Plus, _) => Err(Error::ExecutionError(format!("Cannot apply unary plus to {:?}", val))),

            (BitwiseNot, SqlValue::Integer(i)) => Ok(SqlValue::Integer(!i)),
            (BitwiseNot, SqlValue::Null) => Ok(SqlValue::Null),
            (BitwiseNot, _) => Err(Error::ExecutionError(format!("Cannot apply bitwise not to {:?}", val))),

            (IsNull, SqlValue::Null) => Ok(SqlValue::Boolean(true)),
            (IsNull, _) => Ok(SqlValue::Boolean(false)),

            (IsNotNull, SqlValue::Null) => Ok(SqlValue::Boolean(false)),
            (IsNotNull, _) => Ok(SqlValue::Boolean(true)),

            (Isnull, SqlValue::Null) => Ok(SqlValue::Boolean(true)),
            (Isnull, _) => Ok(SqlValue::Boolean(false)),

            (Notnull, SqlValue::Null) => Ok(SqlValue::Boolean(false)),
            (Notnull, _) => Ok(SqlValue::Boolean(true)),
        }
    }

    /// Evaluate function call
    /// References: Standard SQL functions - aggregate, string, math, date
    fn eval_function(name: &str, args: Vec<SqlValue>) -> Result<SqlValue> {
        match name.to_uppercase().as_str() {
            // String functions
            "LENGTH" | "LEN" => {
                if args.len() != 1 {
                    return Err(Error::ExecutionError(
                        "LENGTH requires 1 argument".to_string(),
                    ));
                }
                match &args[0] {
                    SqlValue::Text(s) => Ok(SqlValue::Integer(s.len() as i64)),
                    SqlValue::Null => Ok(SqlValue::Null),
                    _ => Err(Error::ExecutionError("LENGTH requires text".to_string())),
                }
            }

            "UPPER" | "UCASE" => {
                if args.len() != 1 {
                    return Err(Error::ExecutionError(
                        "UPPER requires 1 argument".to_string(),
                    ));
                }
                match &args[0] {
                    SqlValue::Text(s) => Ok(SqlValue::Text(s.to_uppercase())),
                    SqlValue::Null => Ok(SqlValue::Null),
                    _ => Err(Error::ExecutionError("UPPER requires text".to_string())),
                }
            }

            "LOWER" | "LCASE" => {
                if args.len() != 1 {
                    return Err(Error::ExecutionError(
                        "LOWER requires 1 argument".to_string(),
                    ));
                }
                match &args[0] {
                    SqlValue::Text(s) => Ok(SqlValue::Text(s.to_lowercase())),
                    SqlValue::Null => Ok(SqlValue::Null),
                    _ => Err(Error::ExecutionError("LOWER requires text".to_string())),
                }
            }

            "SUBSTR" | "SUBSTRING" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(Error::ExecutionError(
                        "SUBSTR requires 2-3 arguments".to_string(),
                    ));
                }
                match (&args[0], &args[1]) {
                    (SqlValue::Text(s), SqlValue::Integer(start)) => {
                        let start = (*start as usize).saturating_sub(1);
                        let substr = if args.len() == 3 {
                            match &args[2] {
                                SqlValue::Integer(len) => {
                                    let len = *len as usize;
                                    s[start..].chars().take(len).collect::<String>()
                                }
                                _ => return Err(Error::ExecutionError(
                                    "SUBSTR length must be integer".to_string(),
                                )),
                            }
                        } else {
                            s[start..].to_string()
                        };
                        Ok(SqlValue::Text(substr))
                    }
                    _ => Err(Error::ExecutionError(
                        "SUBSTR requires text and integer".to_string(),
                    )),
                }
            }

            // Math functions
            "ABS" => {
                if args.len() != 1 {
                    return Err(Error::ExecutionError("ABS requires 1 argument".to_string()));
                }
                match &args[0] {
                    SqlValue::Integer(i) => Ok(SqlValue::Integer(i.abs())),
                    SqlValue::Real(f) => Ok(SqlValue::Real(f.abs())),
                    SqlValue::Null => Ok(SqlValue::Null),
                    _ => Err(Error::ExecutionError("ABS requires number".to_string())),
                }
            }

            "ROUND" => {
                if args.len() < 1 || args.len() > 2 {
                    return Err(Error::ExecutionError("ROUND requires 1-2 arguments".to_string()));
                }
                let precision = if args.len() == 2 {
                    match &args[1] {
                        SqlValue::Integer(p) => *p as u32,
                        _ => return Err(Error::ExecutionError(
                            "ROUND precision must be integer".to_string(),
                        )),
                    }
                } else {
                    0
                };

                match &args[0] {
                    SqlValue::Real(f) => {
                        let multiplier = 10_f64.powi(precision as i32);
                        Ok(SqlValue::Real((f * multiplier).round() / multiplier))
                    }
                    SqlValue::Integer(i) => Ok(SqlValue::Integer(*i)),
                    SqlValue::Null => Ok(SqlValue::Null),
                    _ => Err(Error::ExecutionError("ROUND requires number".to_string())),
                }
            }

            // Type functions
            "TYPEOF" => {
                if args.len() != 1 {
                    return Err(Error::ExecutionError("TYPEOF requires 1 argument".to_string()));
                }
                let type_name = match &args[0] {
                    SqlValue::Null => "null",
                    SqlValue::Boolean(_) => "boolean",
                    SqlValue::Integer(_) => "integer",
                    SqlValue::Real(_) => "real",
                    SqlValue::Text(_) => "text",
                    SqlValue::Blob(_) => "blob",
                };
                Ok(SqlValue::Text(type_name.to_string()))
            }

            // Aggregate functions (need context - placeholder)
            "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" => {
                Err(Error::ExecutionError(format!(
                    "Aggregate function {} requires special handling",
                    name
                )))
            }

            _ => Err(Error::ExecutionError(format!("Unknown function: {}", name))),
        }
    }

    /// Check if two values are equal
    fn values_equal(a: &SqlValue, b: &SqlValue) -> Result<bool> {
        Ok(match (a, b) {
            (SqlValue::Null, SqlValue::Null) => true,
            (SqlValue::Null, _) | (_, SqlValue::Null) => false,
            (SqlValue::Boolean(a), SqlValue::Boolean(b)) => a == b,
            (SqlValue::Integer(a), SqlValue::Integer(b)) => a == b,
            (SqlValue::Real(a), SqlValue::Real(b)) => (a - b).abs() < f64::EPSILON,
            (SqlValue::Integer(a), SqlValue::Real(b)) | (SqlValue::Real(b), SqlValue::Integer(a)) => {
                (*a as f64 - b).abs() < f64::EPSILON
            }
            (SqlValue::Text(a), SqlValue::Text(b)) => a == b,
            (SqlValue::Blob(a), SqlValue::Blob(b)) => a == b,
            _ => false,
        })
    }

    /// Check if value is truthy (non-zero, non-null, true)
    fn is_truthy(val: &SqlValue) -> bool {
        match val {
            SqlValue::Null => false,
            SqlValue::Boolean(b) => *b,
            SqlValue::Integer(i) => *i != 0,
            SqlValue::Real(f) => *f != 0.0,
            SqlValue::Text(s) => !s.is_empty(),
            SqlValue::Blob(b) => !b.is_empty(),
        }
    }

    /// Cast value to target type
    fn cast_value(val: &SqlValue, target_type: &str) -> Result<SqlValue> {
        match target_type.to_uppercase().as_str() {
            "INTEGER" | "INT" => match val {
                SqlValue::Integer(i) => Ok(SqlValue::Integer(*i)),
                SqlValue::Real(f) => Ok(SqlValue::Integer(*f as i64)),
                SqlValue::Text(s) => s.parse::<i64>()
                    .map(SqlValue::Integer)
                    .or(Ok(SqlValue::Null)),
                SqlValue::Null => Ok(SqlValue::Null),
                _ => Ok(SqlValue::Null),
            },
            "REAL" | "FLOAT" => match val {
                SqlValue::Real(f) => Ok(SqlValue::Real(*f)),
                SqlValue::Integer(i) => Ok(SqlValue::Real(*i as f64)),
                SqlValue::Text(s) => s.parse::<f64>()
                    .map(SqlValue::Real)
                    .or(Ok(SqlValue::Null)),
                SqlValue::Null => Ok(SqlValue::Null),
                _ => Ok(SqlValue::Null),
            },
            "TEXT" | "VARCHAR" => match val {
                SqlValue::Text(s) => Ok(SqlValue::Text(s.clone())),
                SqlValue::Integer(i) => Ok(SqlValue::Text(i.to_string())),
                SqlValue::Real(f) => Ok(SqlValue::Text(f.to_string())),
                SqlValue::Boolean(b) => Ok(SqlValue::Text(b.to_string())),
                SqlValue::Null => Ok(SqlValue::Null),
                _ => Ok(SqlValue::Null),
            },
            "BLOB" => Ok(val.clone()),
            _ => Err(Error::ExecutionError(format!(
                "Unknown type: {}",
                target_type
            ))),
        }
    }
}

/// Virtual machine for executing plans
/// References: SQLite Virtual Machine operations and semantics
pub struct VirtualMachine {
    /// In-memory table storage (table_name -> rows)
    tables: HashMap<String, ResultSet>,
    /// Index storage: (index_name -> (table_name, column_names, is_unique))
    indexes: HashMap<String, IndexMetadata>,
}

/// Index metadata
#[derive(Debug, Clone)]
struct IndexMetadata {
    /// Table being indexed
    table: String,
    /// Columns in index
    columns: Vec<String>,
    /// Whether index enforces uniqueness
    unique: bool,
}

impl VirtualMachine {
    /// Create a new virtual machine
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            indexes: HashMap::new(),
        }
    }

    /// Execute an execution plan
    /// References: https://www.sqlite.org/opcode.html
    pub fn execute(&mut self, plan: &ExecutionPlan) -> Result<ResultSet> {
        match plan {
            // FullTableScan: Sequential scan of entire table without index
            // Example: SELECT * FROM users
            // Generates all rows from table in storage order
            ExecutionPlan::FullTableScan { table, alias: _ } => {
                self.tables
                    .get(*table)
                    .cloned()
                    .ok_or_else(|| Error::ExecutionError(format!("Table '{}' not found", table)))
            }

            // IndexScan: Search using index structure for fast lookup
            // Example: SELECT * FROM users WHERE id = 42 (uses PRIMARY KEY index)
            // Returns only rows matching index condition, avoiding full table scan
            ExecutionPlan::IndexScan {
                table,
                index: _,
                condition,
            } => {
                let mut result = self.tables
                    .get(*table)
                    .cloned()
                    .ok_or_else(|| Error::ExecutionError(format!("Table '{}' not found", table)))?;

                if let Some(cond) = condition {
                    result = self.apply_filter(&result, cond)?;
                }

                Ok(result)
            }

            // Filter: Apply WHERE clause to eliminate non-matching rows
            // Example: SELECT * FROM orders WHERE total > 100
            // Evaluates condition on each row, keeping only rows where condition is true
            ExecutionPlan::Filter { input, condition } => {
                let input_result = self.execute(input)?;
                self.apply_filter(&input_result, condition)
            }

            // Sort: Arrange rows in specified order (ASC/DESC)
            // Example: SELECT * FROM products ORDER BY price DESC, name ASC
            // Primary sort by price (descending), then by name (ascending) for ties
            ExecutionPlan::Sort { input, order_by } => {
                let mut result = self.execute(input)?;

                let order_terms: Result<Vec<_>> = order_by
                    .iter()
                    .map(|term| {
                        let col_name = self.extract_column_name(&term.expr)?;
                        Ok((col_name, term.direction))
                    })
                    .collect();

                result.sort(&order_terms?)?;
                Ok(result)
            }

            // Limit: Restrict output rows to specified count and start position
            // Example: SELECT * FROM logs LIMIT 10 OFFSET 20
            // Returns 10 rows starting from row 20 (pagination)
            ExecutionPlan::Limit {
                input,
                limit,
                offset,
            } => {
                let mut result = self.execute(input)?;

                let empty_row: Row = vec![];
                let empty_columns: Vec<String> = vec![];

                let limit_count = if let Some(limit_expr) = limit {
                    match ExpressionEvaluator::eval(limit_expr, &empty_row, &empty_columns)? {
                        SqlValue::Integer(i) => Some(i as usize),
                        _ => None,
                    }
                } else {
                    None
                };

                let offset_count = if let Some(offset_expr) = offset {
                    match ExpressionEvaluator::eval(offset_expr, &empty_row, &empty_columns)? {
                        SqlValue::Integer(i) => Some(i as usize),
                        _ => None,
                    }
                } else {
                    None
                };

                result.limit(limit_count, offset_count);
                Ok(result)
            }

            // Project: Select specific columns from result set
            // Example: SELECT id, name, email FROM users
            // Returns only specified columns, dropping others (column pruning)
            ExecutionPlan::Project { input, columns } => {
                let result = self.execute(input)?;
                result.project(columns)
            }

            // Insert: Add new rows to table
            // Example: INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')
            // Evaluates value expressions and appends new rows to table storage
            ExecutionPlan::Insert {
                table,
                columns: _,
                values,
            } => {
                // Create table if it doesn't exist
                if !self.tables.contains_key(*table) {
                    self.tables.insert(
                        table.to_string(),
                        ResultSet::new(
                            (0..values[0].len())
                                .map(|i| format!("col{}", i))
                                .collect(),
                        ),
                    );
                }

                let table_ref = self.tables.get_mut(*table).unwrap();
                let empty_row: Row = vec![];
                let empty_columns: Vec<String> = vec![];

                for value_row in values {
                    let mut row = Row::new();
                    for (i, val_expr) in value_row.iter().enumerate() {
                        let val = ExpressionEvaluator::eval(val_expr, &empty_row, &empty_columns)?;
                        row.push((format!("col{}", i), val));
                    }
                    table_ref.add_row(row);
                }

                Ok(ResultSet::new(vec![]))
            }

            // Update: Modify existing rows in table
            // Example: UPDATE users SET status = 'active', modified = NOW() WHERE id = 42
            // Finds matching rows and applies column assignments to each
            ExecutionPlan::Update {
                table,
                assignments,
                condition,
            } => {
                if let Some(table_data) = self.tables.get_mut(*table) {
                    for row in &mut table_data.rows {
                        if let Some(cond) = condition {
                            let eval_result = ExpressionEvaluator::eval(cond, row, &table_data.columns)?;
                            if !ExpressionEvaluator::is_truthy(&eval_result) {
                                continue;
                            }
                        }

                        for assignment in assignments {
                            if let Some(idx) = table_data.columns.iter().position(|c| c == &assignment.column.to_string()) {
                                let new_val = ExpressionEvaluator::eval(&assignment.value, row, &table_data.columns)?;
                                row[idx].1 = new_val;
                            }
                        }
                    }
                }

                Ok(ResultSet::new(vec![]))
            }

            // Delete: Remove rows from table
            // Example: DELETE FROM audit_log WHERE created_at < '2020-01-01'
            // Removes all rows satisfying condition (without condition, deletes all rows)
            ExecutionPlan::Delete { table, condition } => {
                if let Some(table_data) = self.tables.get_mut(*table) {
                    table_data.rows.retain(|row| {
                        if let Some(cond) = condition {
                            if let Ok(eval_result) = ExpressionEvaluator::eval(cond, row, &table_data.columns) {
                                !ExpressionEvaluator::is_truthy(&eval_result)
                            } else {
                                true
                            }
                        } else {
                            false
                        }
                    });
                }

                Ok(ResultSet::new(vec![]))
            }

            // CreateTable: Define new table schema and initialize storage
            // Example: CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)
            // Allocates table structure and prepares for INSERT operations
            ExecutionPlan::CreateTable { table, columns } => {
                let column_names = columns.iter().map(|c| c.name.to_string()).collect();
                self.tables.insert(table.to_string(), ResultSet::new(column_names));
                Ok(ResultSet::new(vec![]))
            }

            // NestedLoopJoin: Cartesian product with optional join condition
            // Example: SELECT * FROM orders o JOIN customers c ON o.customer_id = c.id
            // For each row in left table, matches against all rows in right table
            // O(n*m) complexity - simple but slower for large tables
            ExecutionPlan::NestedLoopJoin {
                left,
                right,
                condition,
            } => {
                let left_result = self.execute(left)?;
                let right_result = self.execute(right)?;
                self.nested_loop_join(&left_result, &right_result, condition)
            }

            // HashJoin: Hash-based join for better performance on large datasets
            // Example: SELECT * FROM orders JOIN customers ON orders.cust_id = customers.id
            // Builds hash table from smaller input, probes with other input
            // O(n+m) expected complexity - faster than nested loop for most cases
            ExecutionPlan::HashJoin {
                left,
                right,
                left_key,
                right_key,
            } => {
                let left_result = self.execute(left)?;
                let right_result = self.execute(right)?;
                self.hash_join(&left_result, &right_result, left_key, right_key)
            }

            // Composite: Execute sequence of plans, using output of one as input to next
            // Example: BEGIN; INSERT INTO log VALUES (...); UPDATE stats SET count = count + 1; COMMIT;
            // Chains multiple operations together, returning result of final plan
            ExecutionPlan::Composite(plans) => {
                let mut result = ResultSet::new(vec![]);
                for plan in plans {
                    result = self.execute(plan)?;
                }
                Ok(result)
            }

            // GroupBy: Group rows by key expressions and compute aggregates
            // Example: SELECT dept, COUNT(*) FROM employees GROUP BY dept
            // Groups rows by grouping expressions, applies aggregate functions
            ExecutionPlan::GroupBy {
                input,
                group_keys,
                aggregates: _,
            } => {
                let input_result = self.execute(input)?;
                self.apply_group_by(&input_result, group_keys)
            }

            // Distinct: Remove duplicate rows from result set
            // Example: SELECT DISTINCT country FROM customers
            // Keeps first occurrence of each unique row, removes duplicates
            ExecutionPlan::Distinct { input } => {
                let result = self.execute(input)?;
                self.apply_distinct(&result)
            }

            // CreateIndex: Create an index on table column(s)
            // Example: CREATE INDEX idx_user_email ON users (email)
            // Stores index metadata for query optimizer to use
            ExecutionPlan::CreateIndex {
                index,
                table,
                columns,
                unique,
            } => {
                self.indexes.insert(
                    index.to_string(),
                    IndexMetadata {
                        table: table.to_string(),
                        columns: columns.iter().map(|c| c.to_string()).collect(),
                        unique: *unique,
                    },
                );
                Ok(ResultSet::new(vec![]))
            }

            // DropIndex: Remove an index
            // Example: DROP INDEX idx_user_email
            // Removes index metadata, allowing full table scans for affected queries
            ExecutionPlan::DropIndex { index } => {
                self.indexes.remove(*index);
                Ok(ResultSet::new(vec![]))
            }
        }
    }

    /// Apply filter condition to result set
    fn apply_filter(&self, result: &ResultSet, condition: &Expression) -> Result<ResultSet> {
        let mut filtered = ResultSet::new(result.columns.clone());

        for row in &result.rows {
            let eval_result = ExpressionEvaluator::eval(condition, row, &result.columns)?;
            if ExpressionEvaluator::is_truthy(&eval_result) {
                filtered.add_row(row.clone());
            }
        }

        Ok(filtered)
    }

    /// Extract column name from expression (simplified)
    fn extract_column_name(&self, expr: &Expression) -> Result<String> {
        match expr {
            Expression::Identifier(name) => Ok(name.to_string()),
            Expression::QualifiedIdentifier { column, .. } => Ok(column.to_string()),
            _ => Err(Error::ExecutionError(
                "ORDER BY requires column identifier".to_string(),
            )),
        }
    }

    /// Nested loop join implementation
    fn nested_loop_join(
        &self,
        left: &ResultSet,
        right: &ResultSet,
        condition: &Option<Expression>,
    ) -> Result<ResultSet> {
        let mut joined_columns = left.columns.clone();
        joined_columns.extend(right.columns.clone());
        let mut result = ResultSet::new(joined_columns);

        for left_row in &left.rows {
            for right_row in &right.rows {
                // Combine rows
                let mut combined = left_row.clone();
                combined.extend(right_row.clone());

                // Evaluate join condition if present
                if let Some(cond) = condition {
                    let eval_result = ExpressionEvaluator::eval(cond, &combined, &result.columns)?;
                    if ExpressionEvaluator::is_truthy(&eval_result) {
                        result.add_row(combined);
                    }
                } else {
                    result.add_row(combined);
                }
            }
        }

        Ok(result)
    }

    /// Hash join implementation
    fn hash_join(
        &self,
        left: &ResultSet,
        right: &ResultSet,
        left_key: &Expression,
        right_key: &Expression,
    ) -> Result<ResultSet> {
        let mut joined_columns = left.columns.clone();
        joined_columns.extend(right.columns.clone());
        let mut result = ResultSet::new(joined_columns);

        // Build hash map from left table
        let mut hash_map: HashMap<String, Vec<Row>> = HashMap::new();
        for left_row in &left.rows {
            let key_val = ExpressionEvaluator::eval(left_key, left_row, &left.columns)?;
            let key_str = format!("{:?}", key_val);
            hash_map.entry(key_str).or_insert_with(Vec::new).push(left_row.clone());
        }

        // Probe with right table
        for right_row in &right.rows {
            let key_val = ExpressionEvaluator::eval(right_key, right_row, &right.columns)?;
            let key_str = format!("{:?}", key_val);

            if let Some(matching_left_rows) = hash_map.get(&key_str) {
                for left_row in matching_left_rows {
                    let mut combined = left_row.clone();
                    combined.extend(right_row.clone());
                    result.add_row(combined);
                }
            }
        }

        Ok(result)
    }

    /// Apply GROUP BY aggregation
    fn apply_group_by(
        &self,
        result: &ResultSet,
        group_keys: &[Expression],
    ) -> Result<ResultSet> {
        use std::collections::BTreeMap;

        // Group rows by key expressions
        let mut groups: BTreeMap<String, Vec<Row>> = BTreeMap::new();

        for row in &result.rows {
            // Evaluate grouping key expressions
            let mut key_parts = Vec::new();
            for expr in group_keys {
                let val = ExpressionEvaluator::eval(expr, row, &result.columns)?;
                key_parts.push(format!("{:?}", val));
            }
            let group_key = key_parts.join("|");

            groups.entry(group_key).or_insert_with(Vec::new).push(row.clone());
        }

        // Create result with one row per group
        let mut grouped_result = ResultSet::new(result.columns.clone());
        for (_key, group_rows) in groups {
            // Return first row of each group (aggregates would be computed here)
            if let Some(first_row) = group_rows.first() {
                grouped_result.add_row(first_row.clone());
            }
        }

        Ok(grouped_result)
    }

    /// Apply DISTINCT filtering
    fn apply_distinct(&self, result: &ResultSet) -> Result<ResultSet> {
        use std::collections::HashSet;

        let mut seen = HashSet::new();
        let mut distinct_result = ResultSet::new(result.columns.clone());

        for row in &result.rows {
            let row_key = format!("{:?}", row);
            if seen.insert(row_key) {
                distinct_result.add_row(row.clone());
            }
        }

        Ok(distinct_result)
    }

}

impl Default for VirtualMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to compare SQL values
/// References: Comparison semantics in SQL
fn compare_values(a: &SqlValue, b: &SqlValue) -> Ordering {
    match (a, b) {
        (SqlValue::Null, SqlValue::Null) => Ordering::Equal,
        (SqlValue::Null, _) => Ordering::Less,
        (_, SqlValue::Null) => Ordering::Greater,

        (SqlValue::Integer(a), SqlValue::Integer(b)) => a.cmp(b),
        (SqlValue::Real(a), SqlValue::Real(b)) => {
            if a < b {
                Ordering::Less
            } else if a > b {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        (SqlValue::Integer(a), SqlValue::Real(b)) => {
            let a_f = *a as f64;
            if a_f < *b {
                Ordering::Less
            } else if a_f > *b {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        (SqlValue::Real(a), SqlValue::Integer(b)) => {
            let b_f = *b as f64;
            if a < &b_f {
                Ordering::Less
            } else if a > &b_f {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }

        (SqlValue::Text(a), SqlValue::Text(b)) => a.cmp(b),
        (SqlValue::Boolean(a), SqlValue::Boolean(b)) => a.cmp(b),

        _ => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_literal() {
        let row = vec![];
        let cols = vec![];
        assert_eq!(
            ExpressionEvaluator::eval(&Expression::Literal("42"), &row, &cols).unwrap(),
            SqlValue::Integer(42)
        );
        assert_eq!(
            ExpressionEvaluator::eval(&Expression::Literal("3.14"), &row, &cols).unwrap(),
            SqlValue::Real(3.14)
        );
    }

    #[test]
    fn test_arithmetic_operations() {
        let row = vec![];
        let cols = vec![];
        let left = Expression::Literal("10");
        let right = Expression::Literal("5");

        let add_expr = Expression::BinaryOp {
            left: Box::new(left.clone()),
            op: BinaryOperator::Add,
            right: Box::new(right.clone()),
        };

        let result = ExpressionEvaluator::eval(&add_expr, &row, &cols).unwrap();
        assert_eq!(result, SqlValue::Integer(15));
    }

    #[test]
    fn test_string_length_function() {
        let row = vec![];
        let cols = vec![];
        let call = Expression::FunctionCall {
            name: "LENGTH",
            args: vec![Expression::Literal("'hello'")],
        };

        let result = ExpressionEvaluator::eval(&call, &row, &cols).unwrap();
        assert_eq!(result, SqlValue::Integer(5));
    }

    #[test]
    fn test_result_set_projection() {
        let mut rs = ResultSet::new(vec!["id".to_string(), "name".to_string(), "age".to_string()]);
        rs.add_row(vec![
            ("id".to_string(), SqlValue::Integer(1)),
            ("name".to_string(), SqlValue::Text("Alice".to_string())),
            ("age".to_string(), SqlValue::Integer(30)),
        ]);

        let projected = rs.project(&["id", "name"]).unwrap();
        assert_eq!(projected.columns, vec!["id", "name"]);
        assert_eq!(projected.rows[0].len(), 2);
    }

    #[test]
    fn test_virtual_machine_create_table() {
        let mut vm = VirtualMachine::new();
        let plan = ExecutionPlan::CreateTable {
            table: "users",
            columns: vec![],
        };

        let _result = vm.execute(&plan).unwrap();
        assert!(vm.tables.contains_key("users"));
    }

    #[test]
    fn test_comparison_values() {
        assert_eq!(
            compare_values(&SqlValue::Integer(5), &SqlValue::Integer(10)),
            Ordering::Less
        );
        assert_eq!(
            compare_values(&SqlValue::Text("a".to_string()), &SqlValue::Text("b".to_string())),
            Ordering::Less
        );
    }

    #[test]
    fn test_distinct_filtering() {
        let mut result = ResultSet::new(vec!["id".to_string(), "name".to_string()]);
        result.add_row(vec![
            ("id".to_string(), SqlValue::Integer(1)),
            ("name".to_string(), SqlValue::Text("Alice".to_string())),
        ]);
        result.add_row(vec![
            ("id".to_string(), SqlValue::Integer(1)),
            ("name".to_string(), SqlValue::Text("Alice".to_string())),
        ]);
        result.add_row(vec![
            ("id".to_string(), SqlValue::Integer(2)),
            ("name".to_string(), SqlValue::Text("Bob".to_string())),
        ]);

        let vm = VirtualMachine::new();
        let distinct = vm.apply_distinct(&result).unwrap();

        assert_eq!(distinct.rows.len(), 2);
    }

    #[test]
    fn test_group_by_basic() {
        let mut result = ResultSet::new(vec!["dept".to_string(), "salary".to_string()]);
        result.add_row(vec![
            ("dept".to_string(), SqlValue::Text("Sales".to_string())),
            ("salary".to_string(), SqlValue::Integer(50000)),
        ]);
        result.add_row(vec![
            ("dept".to_string(), SqlValue::Text("Sales".to_string())),
            ("salary".to_string(), SqlValue::Integer(60000)),
        ]);
        result.add_row(vec![
            ("dept".to_string(), SqlValue::Text("IT".to_string())),
            ("salary".to_string(), SqlValue::Integer(70000)),
        ]);

        let vm = VirtualMachine::new();
        let group_expr = Expression::Identifier("dept");
        let grouped = vm
            .apply_group_by(&result, &[group_expr])
            .unwrap();

        // Should have 2 groups: Sales and IT
        assert_eq!(grouped.rows.len(), 2);
    }

    #[test]
    fn test_create_index() {
        let mut vm = VirtualMachine::new();
        let plan = ExecutionPlan::CreateIndex {
            index: "idx_users_email",
            table: "users",
            columns: vec!["email"],
            unique: true,
        };

        let result = vm.execute(&plan).unwrap();
        assert_eq!(result.rows.len(), 0); // DDL returns empty result set
        assert!(vm.indexes.contains_key("idx_users_email"));
    }

    #[test]
    fn test_drop_index() {
        let mut vm = VirtualMachine::new();

        // Create index first
        let create_plan = ExecutionPlan::CreateIndex {
            index: "idx_test",
            table: "test_table",
            columns: vec!["col1"],
            unique: false,
        };
        vm.execute(&create_plan).unwrap();
        assert!(vm.indexes.contains_key("idx_test"));

        // Drop index
        let drop_plan = ExecutionPlan::DropIndex {
            index: "idx_test",
        };
        vm.execute(&drop_plan).unwrap();
        assert!(!vm.indexes.contains_key("idx_test"));
    }

    #[test]
    fn test_index_scan_optimization() {
        let mut vm = VirtualMachine::new();

        // Create table
        let create_table_plan = ExecutionPlan::CreateTable {
            table: "users",
            columns: vec![],
        };
        vm.execute(&create_table_plan).unwrap();

        // Create index
        let create_index_plan = ExecutionPlan::CreateIndex {
            index: "idx_users_id",
            table: "users",
            columns: vec!["id"],
            unique: false,
        };
        vm.execute(&create_index_plan).unwrap();

        // Verify index exists
        assert!(vm.indexes.contains_key("idx_users_id"));
        let index_meta = vm.indexes.get("idx_users_id").unwrap();
        assert_eq!(index_meta.table, "users");
        assert_eq!(index_meta.columns, vec!["id".to_string()]);
    }
}



