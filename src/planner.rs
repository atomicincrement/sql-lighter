//! Query Planner and Optimizer (Phase 4d)
//!
//! Transforms parsed SQL statements into optimized execution plans.
//! References:
//! - SQLite Query Optimizer: https://www.sqlite.org/optoverview.html
//! - Query Planner: https://www.sqlite.org/queryplanner.html
//! - Cost-based Query Optimization: https://use-the-index-luke.com/
//! - Traditional approach from "Database System Concepts" (Silberschatz et al.)

use crate::error::{Error, Result};
use crate::parser::{
    BinaryOperator, ColumnDef, Expression, OrderingTerm, SelectStatement,
    Statement, TableOrSubquery, UpdateAssignment,
};
use std::collections::HashMap;

/// Execution plan for a SQL statement
#[derive(Debug, Clone)]
pub enum ExecutionPlan<'a> {
    /// Scan an entire table
    /// Example: `SELECT * FROM users`
    FullTableScan {
        table: &'a str,
        alias: Option<&'a str>,
    },
    /// Index scan (future optimization)
    /// Example: `SELECT * FROM users WHERE id = 42` (with index on id)
    IndexScan {
        table: &'a str,
        index: &'a str,
        condition: Option<Expression<'a>>,
    },
    /// Nested loop join - reference: https://use-the-index-luke.com/sql/join
    /// Example: `SELECT * FROM users u JOIN orders o ON u.id = o.user_id`
    NestedLoopJoin {
        left: Box<ExecutionPlan<'a>>,
        right: Box<ExecutionPlan<'a>>,
        condition: Option<Expression<'a>>,
    },
    /// Hash join - for larger datasets
    /// Example: `SELECT * FROM large_table1 JOIN large_table2 ON t1.key = t2.key`
    HashJoin {
        left: Box<ExecutionPlan<'a>>,
        right: Box<ExecutionPlan<'a>>,
        left_key: Expression<'a>,
        right_key: Expression<'a>,
    },
    /// Filter rows based on predicate
    /// Example: `SELECT * FROM users WHERE age > 18 AND status = 'active'`
    Filter {
        input: Box<ExecutionPlan<'a>>,
        condition: Expression<'a>,
    },
    /// Sort rows
    /// Example: `SELECT * FROM products ORDER BY price DESC, name ASC`
    Sort {
        input: Box<ExecutionPlan<'a>>,
        order_by: Vec<OrderingTerm<'a>>,
    },
    /// Limit and offset results
    /// Example: `SELECT * FROM users LIMIT 10 OFFSET 20` (pagination)
    Limit {
        input: Box<ExecutionPlan<'a>>,
        limit: Option<Expression<'a>>,
        offset: Option<Expression<'a>>,
    },
    /// Project specific columns
    /// Example: `SELECT id, name, email FROM users`
    Project {
        input: Box<ExecutionPlan<'a>>,
        columns: Vec<&'a str>,
    },
    /// Insert rows into table
    /// Example: `INSERT INTO users (id, name) VALUES (1, 'Alice')`
    Insert {
        table: &'a str,
        columns: Option<Vec<&'a str>>,
        values: Vec<Vec<Expression<'a>>>,
    },
    /// Update rows in table
    /// Example: `UPDATE users SET status = 'inactive' WHERE last_login < '2024-01-01'`
    Update {
        table: &'a str,
        assignments: Vec<UpdateAssignment<'a>>,
        condition: Option<Expression<'a>>,
    },
    /// Delete rows from table
    /// Example: `DELETE FROM logs WHERE created_at < '2023-01-01'`
    Delete {
        table: &'a str,
        condition: Option<Expression<'a>>,
    },
    /// Create a new table
    /// Example: `CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)`
    CreateTable {
        table: &'a str,
        columns: Vec<ColumnDef<'a>>,
    },
    /// Group by aggregation - groups rows and applies aggregate functions
    /// Example: `SELECT dept, COUNT(*) FROM employees GROUP BY dept`
    GroupBy {
        input: Box<ExecutionPlan<'a>>,
        group_keys: Vec<Expression<'a>>,
        aggregates: Vec<(&'a str, AggregateFunction<'a>)>,
    },
    /// Distinct filtering - removes duplicate rows
    /// Example: `SELECT DISTINCT country FROM customers`
    Distinct {
        input: Box<ExecutionPlan<'a>>,
    },
    /// Create index on table columns
    /// Example: `CREATE INDEX idx_user_email ON users (email)`
    CreateIndex {
        index: &'a str,
        table: &'a str,
        columns: Vec<&'a str>,
        unique: bool,
    },
    /// Drop index
    /// Example: `DROP INDEX idx_user_email`
    DropIndex {
        index: &'a str,
    },
    /// Composite plan (for multiple operations)
    /// Example: Transaction with multiple statements: BEGIN; INSERT ...; UPDATE ...; COMMIT;
    Composite(Vec<ExecutionPlan<'a>>),
}

/// Aggregate function specification
#[derive(Debug, Clone)]
pub enum AggregateFunction<'a> {
    /// COUNT(*) or COUNT(expr)
    Count(Option<Expression<'a>>),
    /// SUM(expr)
    Sum(Expression<'a>),
    /// AVG(expr)
    Avg(Expression<'a>),
    /// MIN(expr)
    Min(Expression<'a>),
    /// MAX(expr)
    Max(Expression<'a>),
}

/// Statistics for cost estimation
/// References: Query Optimization statistics from SQLite
#[derive(Debug, Clone, Copy)]
pub struct TableStats {
    /// Estimated number of rows in table
    pub estimated_rows: usize,
    /// Estimated size in bytes
    pub estimated_size: usize,
}

impl Default for TableStats {
    fn default() -> Self {
        Self {
            // Conservative defaults for unknown tables
            estimated_rows: 1000,
            estimated_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Index definition
/// References: B-tree index structure from SQLite file format
#[derive(Debug, Clone)]
pub struct IndexDef<'a> {
    /// Index name
    pub name: &'a str,
    /// Table being indexed
    pub table: &'a str,
    /// Columns in index (order matters)
    pub columns: Vec<&'a str>,
    /// Whether index enforces uniqueness
    pub unique: bool,
}

/// Query planner - converts AST to execution plans
/// References: SQLite query planner - https://www.sqlite.org/queryplanner.html
pub struct Planner<'a> {
    /// Table statistics for cost estimation
    table_stats: HashMap<&'a str, TableStats>,
    /// Available indexes (index_name -> index definition)
    indexes: HashMap<&'a str, IndexDef<'a>>,
    /// Table to indexes mapping (table -> index names)
    table_indexes: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> Planner<'a> {
    /// Create a new planner
    pub fn new() -> Self {
        Self {
            table_stats: HashMap::new(),
            indexes: HashMap::new(),
            table_indexes: HashMap::new(),
        }
    }

    /// Register table statistics for cost estimation
    pub fn with_stats(mut self, table: &'a str, stats: TableStats) -> Self {
        self.table_stats.insert(table, stats);
        self
    }

    /// Register an index on a table
    /// index_name: the index name
    /// table: the table being indexed
    /// columns: the columns that are indexed
    pub fn with_index(
        mut self,
        index_name: &'a str,
        table: &'a str,
        columns: Vec<&'a str>,
    ) -> Self {
        let index_def = IndexDef {
            name: index_name,
            table,
            columns,
            unique: false,
        };
        self.indexes.insert(index_name, index_def);
        self.table_indexes
            .entry(table)
            .or_insert_with(Vec::new)
            .push(index_name);
        self
    }

    /// Plan a statement
    pub fn plan(&self, stmt: &Statement<'a>) -> Result<ExecutionPlan<'a>> {
        match stmt {
            Statement::Select(select) => self.plan_select(select),
            Statement::Insert(insert) => {
                Ok(ExecutionPlan::Insert {
                    table: insert.table,
                    columns: insert.columns.clone(),
                    values: insert.values.clone(),
                })
            }
            Statement::Update(update) => {
                Ok(ExecutionPlan::Update {
                    table: update.table,
                    assignments: update.assignments.clone(),
                    condition: update.where_clause.clone(),
                })
            }
            Statement::Delete(delete) => {
                Ok(ExecutionPlan::Delete {
                    table: delete.table,
                    condition: delete.where_clause.clone(),
                })
            }
            Statement::CreateTable(create) => {
                Ok(ExecutionPlan::CreateTable {
                    table: create.table,
                    columns: create.columns.clone(),
                })
            }
            Statement::DropTableStmt { table } => {
                // For drop table, we just return a delete plan for all rows
                Ok(ExecutionPlan::Delete {
                    table,
                    condition: None,
                })
            }
            Statement::Begin | Statement::Commit | Statement::Rollback => {
                Err(Error::PlanError(
                    "Transaction statements require executor support".to_string(),
                ))
            }
        }
    }

    /// Plan a SELECT statement
    /// References: https://www.sqlite.org/queryplanner.html
    fn plan_select(&self, select: &SelectStatement<'a>) -> Result<ExecutionPlan<'a>> {
        let mut plan: ExecutionPlan<'a>;

        // Step 1: FROM clause - table scan or join
        if let Some(from) = &select.from {
            plan = self.plan_table_source(from)?;
        } else {
            // SELECT without FROM - single row result
            return Ok(ExecutionPlan::Project {
                input: Box::new(ExecutionPlan::FullTableScan {
                    table: "__virtual__",
                    alias: None,
                }),
                columns: vec![], // Will be evaluated in executor
            });
        }

        // Step 2: WHERE clause - filter rows
        // Reference: https://use-the-index-luke.com/sql/where-clause
        if let Some(where_expr) = &select.where_clause {
            // Optimization: check if WHERE can use indexes
            if let Some(index_name) = self.find_usable_index(where_expr, select.from.as_ref().unwrap()) {
                plan = ExecutionPlan::IndexScan {
                    table: select.from.as_ref().unwrap().table,
                    index: index_name,
                    condition: Some(where_expr.clone()),
                };
            } else {
                plan = ExecutionPlan::Filter {
                    input: Box::new(plan),
                    condition: where_expr.clone(),
                };
            }
        }

        // Step 3: GROUP BY clause - aggregate rows
        // Reference: GROUP BY in query planning
        if let Some(group_keys) = &select.group_by {
            // Extract aggregate functions from SELECT columns
            let aggregates = vec![]; // Will be populated from select.columns
            plan = ExecutionPlan::GroupBy {
                input: Box::new(plan),
                group_keys: group_keys.clone(),
                aggregates,
            };
        }

        // Step 4: HAVING clause - filter aggregates (only valid with GROUP BY)
        if let Some(having_expr) = &select.having {
            plan = ExecutionPlan::Filter {
                input: Box::new(plan),
                condition: having_expr.clone(),
            };
        }

        // Step 5: DISTINCT - remove duplicate rows
        if select.distinct {
            plan = ExecutionPlan::Distinct {
                input: Box::new(plan),
            };
        }

        // Step 6: ORDER BY clause
        // Reference: Use index for sorting when possible
        if let Some(order_by) = &select.order_by {
            plan = ExecutionPlan::Sort {
                input: Box::new(plan),
                order_by: order_by.clone(),
            };
        }

        // Step 7: LIMIT / OFFSET
        if select.limit.is_some() || select.offset.is_some() {
            plan = ExecutionPlan::Limit {
                input: Box::new(plan),
                limit: select.limit.clone(),
                offset: select.offset.clone(),
            };
        }

        // Step 8: Projection (column selection)
        let columns = self.extract_columns(&select.columns);
        plan = ExecutionPlan::Project {
            input: Box::new(plan),
            columns,
        };

        Ok(plan)
    }

    /// Plan the FROM clause
    fn plan_table_source(&self, table: &TableOrSubquery<'a>) -> Result<ExecutionPlan<'a>> {
        // Future: handle joins, subqueries, CTEs
        Ok(ExecutionPlan::FullTableScan {
            table: table.table,
            alias: table.alias,
        })
    }

    /// Check if a WHERE condition can use an index and return the index name
    /// Reference: Query optimizer index selection - https://www.sqlite.org/optoverview.html
    fn find_usable_index(
        &self,
        _condition: &Expression<'a>,
        table: &TableOrSubquery<'a>,
    ) -> Option<&'a str> {
        // Get indexes for this table
        if let Some(index_names) = self.table_indexes.get(table.table) {
            // Return first usable index (in real system, would score all indexes)
            // References: Cost-based index selection from https://use-the-index-luke.com/
            return index_names.first().copied();
        }
        None
    }

    /// Extract column names from result columns
    fn extract_columns(&self, columns: &[crate::parser::ResultColumn<'a>]) -> Vec<&'a str> {
        // Placeholder: would expand * to actual columns
        columns
            .iter()
            .filter_map(|col| match col {
                crate::parser::ResultColumn::Star => None,
                crate::parser::ResultColumn::Expression { expr, alias } => {
                    match (expr, alias) {
                        (Expression::Identifier(_col), Some(alias)) => Some(*alias),
                        (Expression::Identifier(col), None) => Some(col),
                        _ => None,
                    }
                }
            })
            .collect()
    }
}

/// Query optimizer - applies transformation rules to improve execution plans
/// References:
/// - SQLite Query Optimizer: https://www.sqlite.org/optoverview.html
/// - Query optimization techniques: https://use-the-index-luke.com/
/// - Traditional database optimization from "Database System Concepts"
pub struct Optimizer;

impl Optimizer {
    /// Create a new optimizer
    pub fn new() -> Self {
        Self
    }

    /// Optimize an execution plan
    pub fn optimize<'a>(&self, plan: ExecutionPlan<'a>) -> ExecutionPlan<'a> {
        // Apply optimization passes
        let mut current = plan;

        // Pass 1: Push filters down to table scans
        current = self.push_filter_down(current);

        // Pass 2: Eliminate redundant operations
        current = self.eliminate_redundancy(current);

        // Pass 3: Combine adjacent operations
        current = self.combine_operations(current);

        current
    }

    /// Push filters down to reduce data processed
    /// Reference: "Selection Pushdown" optimization from database theory
    /// https://use-the-index-luke.com/sql/execution-plans/push-down-optimization
    fn push_filter_down<'a>(&self, plan: ExecutionPlan<'a>) -> ExecutionPlan<'a> {
        match plan {
            ExecutionPlan::Filter {
                input,
                condition,
            } => match *input {
                ExecutionPlan::FullTableScan { table, alias } => {
                    // Convert Filter + FullTableScan to IndexScan if possible
                    ExecutionPlan::Filter {
                        input: Box::new(ExecutionPlan::FullTableScan { table, alias }),
                        condition,
                    }
                }
                ExecutionPlan::Sort {
                    input: sort_input,
                    order_by,
                } => {
                    // Move filter above sort to reduce sorted rows
                    ExecutionPlan::Sort {
                        input: Box::new(ExecutionPlan::Filter {
                            input: sort_input,
                            condition,
                        }),
                        order_by,
                    }
                }
                other => ExecutionPlan::Filter {
                    input: Box::new(other),
                    condition,
                },
            },
            other => other,
        }
    }

    /// Eliminate redundant operations
    /// Reference: "Algebraic Simplification" from database theory
    fn eliminate_redundancy<'a>(&self, plan: ExecutionPlan<'a>) -> ExecutionPlan<'a> {
        match plan {
            ExecutionPlan::Sort { input, order_by } => match *input {
                ExecutionPlan::Limit {
                    input: limit_input,
                    limit,
                    offset,
                } => {
                    // Sort before limit is usually beneficial (though executor decides)
                    ExecutionPlan::Limit {
                        input: Box::new(ExecutionPlan::Sort {
                            input: limit_input,
                            order_by,
                        }),
                        limit,
                        offset,
                    }
                }
                other => ExecutionPlan::Sort {
                    input: Box::new(other),
                    order_by,
                },
            },
            ExecutionPlan::Filter {
                input,
                condition: cond1,
            } => match *input {
                ExecutionPlan::Filter {
                    input: filter_input,
                    condition: cond2,
                } => {
                    // Combine consecutive filters using AND
                    ExecutionPlan::Filter {
                        input: filter_input,
                        condition: Expression::BinaryOp {
                            left: Box::new(cond2),
                            op: BinaryOperator::And,
                            right: Box::new(cond1),
                        },
                    }
                }
                other => ExecutionPlan::Filter {
                    input: Box::new(other),
                    condition: cond1,
                },
            },
            other => other,
        }
    }

    /// Combine adjacent operations when beneficial
    /// Reference: "Operator Combining" optimization technique
    fn combine_operations<'a>(&self, plan: ExecutionPlan<'a>) -> ExecutionPlan<'a> {
        match plan {
            ExecutionPlan::Project { input, columns } => {
                let optimized_input = self.combine_operations(*input);
                ExecutionPlan::Project {
                    input: Box::new(optimized_input),
                    columns,
                }
            }
            ExecutionPlan::Sort {
                input,
                order_by,
            } => {
                let optimized_input = self.combine_operations(*input);
                ExecutionPlan::Sort {
                    input: Box::new(optimized_input),
                    order_by,
                }
            }
            ExecutionPlan::Filter {
                input,
                condition,
            } => {
                let optimized_input = self.combine_operations(*input);
                ExecutionPlan::Filter {
                    input: Box::new(optimized_input),
                    condition,
                }
            }
            ExecutionPlan::NestedLoopJoin {
                left,
                right,
                condition,
            } => {
                let optimized_left = self.combine_operations(*left);
                let optimized_right = self.combine_operations(*right);
                ExecutionPlan::NestedLoopJoin {
                    left: Box::new(optimized_left),
                    right: Box::new(optimized_right),
                    condition,
                }
            }
            other => other,
        }
    }
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn parse_query(sql: &str) -> Result<Statement<'_>> {
        let mut parser = Parser::new(sql)?;
        parser.parse_statement()
    }

    #[test]
    fn test_plan_simple_select() -> Result<()> {
        let stmt = parse_query("SELECT * FROM users")?;
        let planner = Planner::new();
        let plan = planner.plan(&stmt)?;

        // Should produce: Project -> FullTableScan
        match plan {
            ExecutionPlan::Project { .. } => Ok(()),
            _ => Err(Error::PlanError("Expected project plan".to_string())),
        }
    }

    #[test]
    fn test_plan_select_with_where() -> Result<()> {
        let stmt = parse_query("SELECT * FROM users WHERE id = 1")?;
        let planner = Planner::new();
        let plan = planner.plan(&stmt)?;

        // Should produce: Project -> Filter -> FullTableScan
        match plan {
            ExecutionPlan::Project { .. } => Ok(()),
            _ => Err(Error::PlanError("Expected project plan".to_string())),
        }
    }

    #[test]
    fn test_plan_select_with_order_by() -> Result<()> {
        let stmt = parse_query("SELECT * FROM users ORDER BY name ASC")?;
        let planner = Planner::new();
        let plan = planner.plan(&stmt)?;

        // Should produce: Project -> Sort -> FullTableScan
        match plan {
            ExecutionPlan::Project { .. } => Ok(()),
            _ => Err(Error::PlanError("Expected project plan".to_string())),
        }
    }

    #[test]
    fn test_plan_insert() -> Result<()> {
        let stmt = parse_query("INSERT INTO users VALUES (1, 'Alice')")?;
        let planner = Planner::new();
        let plan = planner.plan(&stmt)?;

        match plan {
            ExecutionPlan::Insert { table, .. } => {
                assert_eq!(table, "users");
                Ok(())
            }
            _ => Err(Error::PlanError("Expected insert plan".to_string())),
        }
    }

    #[test]
    fn test_optimizer_combines_filters() {
        let plan = ExecutionPlan::Filter {
            input: Box::new(ExecutionPlan::Filter {
                input: Box::new(ExecutionPlan::FullTableScan {
                    table: "users",
                    alias: None,
                }),
                condition: Expression::Identifier("id"),
            }),
            condition: Expression::Identifier("name"),
        };

        let optimizer = Optimizer::new();
        let optimized = optimizer.optimize(plan);

        // Should combine filters into single filter with AND condition
        match optimized {
            ExecutionPlan::Filter {
                condition: Expression::BinaryOp { op, .. },
                ..
            } if op == BinaryOperator::And => (),
            _ => panic!("Expected combined filter"),
        }
    }

    #[test]
    fn test_table_stats() {
        let stats = TableStats::default();
        assert_eq!(stats.estimated_rows, 1000);
        assert_eq!(stats.estimated_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_plan_select_with_group_by() -> Result<()> {
        let stmt = parse_query("SELECT dept FROM employees GROUP BY dept")?;
        let planner = Planner::new();
        let plan = planner.plan(&stmt)?;

        // Should produce: Project -> GroupBy -> FullTableScan
        match plan {
            ExecutionPlan::Project {
                input: group_plan, ..
            } => match *group_plan {
                ExecutionPlan::GroupBy {
                    input: scan_plan, ..
                } => match *scan_plan {
                    ExecutionPlan::FullTableScan { .. } => Ok(()),
                    _ => Err(Error::PlanError("Expected FullTableScan".to_string())),
                },
                _ => Err(Error::PlanError("Expected GroupBy".to_string())),
            },
            _ => Err(Error::PlanError("Expected Project".to_string())),
        }
    }

    #[test]
    fn test_plan_select_with_distinct() -> Result<()> {
        let stmt = parse_query("SELECT DISTINCT country FROM customers")?;
        let planner = Planner::new();
        let plan = planner.plan(&stmt)?;

        // Should produce: Project -> Distinct -> FullTableScan
        match plan {
            ExecutionPlan::Project {
                input: distinct_plan,
                ..
            } => match *distinct_plan {
                ExecutionPlan::Distinct { input: scan_plan } => match *scan_plan {
                    ExecutionPlan::FullTableScan { .. } => Ok(()),
                    _ => Err(Error::PlanError("Expected FullTableScan".to_string())),
                },
                _ => Err(Error::PlanError("Expected Distinct".to_string())),
            },
            _ => Err(Error::PlanError("Expected Project".to_string())),
        }
    }

    #[test]
    fn test_plan_with_index_scan() -> Result<()> {
        let stmt = parse_query("SELECT * FROM users WHERE id = 1")?;
        let planner = Planner::new()
            .with_index("idx_users_id", "users", vec!["id"]);

        let plan = planner.plan(&stmt)?;

        // Should produce: Project -> IndexScan (due to registered index)
        match plan {
            ExecutionPlan::Project {
                input: index_plan, ..
            } => match *index_plan {
                ExecutionPlan::IndexScan {
                    index,
                    table,
                    condition: Some(_),
                } => {
                    assert_eq!(table, "users");
                    assert_eq!(index, "idx_users_id");
                    Ok(())
                }
                _ => Err(Error::PlanError("Expected IndexScan".to_string())),
            },
            _ => Err(Error::PlanError("Expected Project".to_string())),
        }
    }

    #[test]
    fn test_index_metadata() -> Result<()> {
        let planner = Planner::new()
            .with_index("idx_email", "users", vec!["email"])
            .with_index("idx_name_email", "users", vec!["name", "email"]);

        // Verify planner can find indexes
        let index1 = planner.indexes.get("idx_email");
        assert!(index1.is_some());
        assert_eq!(index1.unwrap().columns, vec!["email"]);

        let index2 = planner.indexes.get("idx_name_email");
        assert!(index2.is_some());
        assert_eq!(index2.unwrap().columns, vec!["name", "email"]);

        Ok(())
    }
}


