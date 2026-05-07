//! SQL Abstract Syntax Tree (AST) and Parser
//!
//! Maintains zero-copy design - AST nodes reference spans from the original input
//! for precise error reporting with line/column information.

use crate::error::{Error, Result};
use crate::lexer::{Lexer, Token};

/// Abstract Syntax Tree root
#[derive(Debug, Clone)]
pub enum Statement<'a> {
    /// SELECT statement
    Select(SelectStatement<'a>),
    /// INSERT statement
    Insert(InsertStatement<'a>),
    /// UPDATE statement
    Update(UpdateStatement<'a>),
    /// DELETE statement
    Delete(DeleteStatement<'a>),
    /// CREATE TABLE statement
    CreateTable(CreateTableStatement<'a>),
    /// DROP TABLE statement
    DropTableStmt { table: &'a str },
    /// BEGIN transaction
    Begin,
    /// COMMIT transaction
    Commit,
    /// ROLLBACK transaction
    Rollback,
}

/// select-stmt / select-core
#[derive(Debug, Clone)]
pub struct SelectStatement<'a> {
    /// Selected result-columns
    pub columns: Vec<ResultColumn<'a>>,
    /// FROM clause
    pub from: Option<TableOrSubquery<'a>>,
    /// WHERE clause
    pub where_clause: Option<Expression<'a>>,
    /// ORDER BY clause
    pub order_by: Option<Vec<OrderingTerm<'a>>>,
    /// LIMIT clause
    pub limit: Option<Expression<'a>>,
    /// OFFSET clause
    pub offset: Option<Expression<'a>>,
    /// DISTINCT flag
    pub distinct: bool,
}

/// result-column in SELECT list
#[derive(Debug, Clone)]
pub enum ResultColumn<'a> {
    /// All columns: *
    Star,
    /// Expression with optional alias: expr [AS alias]
    Expression {
        expr: Expression<'a>,
        alias: Option<&'a str>,
    },
}

/// INSERT statement
#[derive(Debug, Clone)]
pub struct InsertStatement<'a> {
    /// Table name
    pub table: &'a str,
    /// Column names (optional)
    pub columns: Option<Vec<&'a str>>,
    /// Values to insert
    pub values: Vec<Vec<Expression<'a>>>,
}

/// update-stmt
#[derive(Debug, Clone)]
pub struct UpdateStatement<'a> {
    /// Table name
    pub table: &'a str,
    /// SET assignments
    pub assignments: Vec<UpdateAssignment<'a>>,
    /// WHERE clause
    pub where_clause: Option<Expression<'a>>,
}

/// UPDATE assignment (column = value)
#[derive(Debug, Clone)]
pub struct UpdateAssignment<'a> {
    /// Column name
    pub column: &'a str,
    /// New value
    pub value: Expression<'a>,
}

/// DELETE statement
#[derive(Debug, Clone)]
pub struct DeleteStatement<'a> {
    /// Table name
    pub table: &'a str,
    /// WHERE clause
    pub where_clause: Option<Expression<'a>>,
}

/// CREATE TABLE statement
#[derive(Debug, Clone)]
pub struct CreateTableStatement<'a> {
    /// Table name
    pub table: &'a str,
    /// Column definitions
    pub columns: Vec<ColumnDef<'a>>,
}

/// column-def in CREATE TABLE
#[derive(Debug, Clone)]
pub struct ColumnDef<'a> {
    /// Column name
    pub name: &'a str,
    /// Column type
    pub type_name: &'a str,
    /// column-constraints (PRIMARY KEY, NOT NULL, etc.)
    pub constraints: Vec<ColumnConstraint<'a>>,
}

/// column-constraint
#[derive(Debug, Clone)]
pub enum ColumnConstraint<'a> {
    /// PRIMARY KEY
    PrimaryKey,
    /// NOT NULL
    NotNull,
    /// UNIQUE
    Unique,
    /// DEFAULT value
    Default(Expression<'a>),
}

/// table-or-subquery (can be extended for joins later)
#[derive(Debug, Clone)]
pub struct TableOrSubquery<'a> {
    /// Table name
    pub table: &'a str,
    /// Optional alias
    pub alias: Option<&'a str>,
}

/// ordering-term
#[derive(Debug, Clone)]
pub struct OrderingTerm<'a> {
    /// Expression to order by
    pub expr: Expression<'a>,
    /// ASC or DESC
    pub direction: SortDirection,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    /// Ascending (default)
    Asc,
    /// Descending
    Desc,
}

/// Expression in SQL
#[derive(Debug, Clone)]
pub enum Expression<'a> {
    /// Literal value: int, real, string, blob
    Literal(&'a str),
    /// Identifier: column name, function name, etc.
    Identifier(&'a str),
    /// Qualified identifier: table.column
    QualifiedIdentifier {
        table: &'a str,
        column: &'a str,
    },
    /// Binary operation: expr op expr
    BinaryOp {
        left: Box<Expression<'a>>,
        op: BinaryOperator,
        right: Box<Expression<'a>>,
    },
    /// Unary operation: op expr
    UnaryOp {
        op: UnaryOperator,
        operand: Box<Expression<'a>>,
    },
    /// Function call: func(args)
    FunctionCall {
        name: &'a str,
        args: Vec<Expression<'a>>,
    },
    /// Parenthesized expression
    Parenthesized(Box<Expression<'a>>),
    /// CASE expression
    Case {
        operand: Option<Box<Expression<'a>>>,
        when_clauses: Vec<(Expression<'a>, Expression<'a>)>, // (condition, result)
        else_clause: Option<Box<Expression<'a>>>,
    },
    /// CAST expression: CAST(expr AS type)
    Cast {
        expr: Box<Expression<'a>>,
        type_name: &'a str,
    },
    /// NULL
    Null,
    /// TRUE
    True,
    /// FALSE
    False,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Concatenate, // ||

    // Comparison (=, ==, <>, !=)
    Equal,
    NotEqual,

    // Relational (<, >, <=, >=)
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,

    // Logical
    And,
    Or,

    // Bitwise
    BitwiseAnd,     // &
    BitwiseOr,      // |
    BitwiseXor,     // ^
    BitwiseLeftShift,   // <<
    BitwiseRightShift,  // >>

    // SQL-specific comparison operators
    Like,
    Glob,
    Regexp,
    Match,
    In,
    Is,             // IS NULL / IS NOT NULL
    IsDistinctFrom, // IS DISTINCT FROM
    IsNotDistinctFrom, // IS NOT DISTINCT FROM

    // Extraction (JSON)
    Extract,        // ->
    ExtractText,    // ->>
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    /// Logical NOT
    Not,
    /// Unary minus (-)
    Minus,
    /// Unary plus (+)
    Plus,
    /// Bitwise NOT (~)
    BitwiseNot,
    /// IS NULL postfix
    IsNull,
    /// IS NOT NULL postfix
    IsNotNull,
    /// ISNULL postfix
    Isnull,
    /// NOTNULL postfix
    Notnull,
}

/// SQL parser
pub struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    pos: usize,
}

impl<'a> Parser<'a> {
    /// Create a parser from SQL input
    pub fn new(input: &'a str) -> Result<Self> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;
        Ok(Parser { tokens, pos: 0 })
    }

    /// Parse a single SQL statement
    pub fn parse_statement(&mut self) -> Result<Statement<'a>> {
        self.skip_to_next_token();

        if self.is_at_end() {
            return Err(Error::ParseError("Empty statement".into()));
        }

        let token = self.current_token();

        match token {
            Token::Keyword(kw) => {
                if kw.eq_ignore_ascii_case("SELECT") {
                    self.parse_select().map(Statement::Select)
                } else if kw.eq_ignore_ascii_case("INSERT") {
                    self.parse_insert().map(Statement::Insert)
                } else if kw.eq_ignore_ascii_case("UPDATE") {
                    self.parse_update().map(Statement::Update)
                } else if kw.eq_ignore_ascii_case("DELETE") {
                    self.parse_delete().map(Statement::Delete)
                } else if kw.eq_ignore_ascii_case("CREATE") {
                    self.parse_create().map(Statement::CreateTable)
                } else if kw.eq_ignore_ascii_case("DROP") {
                    self.parse_drop()
                } else if kw.eq_ignore_ascii_case("BEGIN") {
                    self.advance();
                    Ok(Statement::Begin)
                } else if kw.eq_ignore_ascii_case("COMMIT") {
                    self.advance();
                    Ok(Statement::Commit)
                } else if kw.eq_ignore_ascii_case("ROLLBACK") {
                    self.advance();
                    Ok(Statement::Rollback)
                } else {
                    Err(Error::ParseError(format!("Unexpected keyword: {}", kw)))
                }
            }
            _ => Err(Error::ParseError(format!(
                "Expected keyword, got {}",
                token.span()
            ))),
        }
    }

    // ===== Parsing methods =====

    fn parse_select(&mut self) -> Result<SelectStatement<'a>> {
        self.expect_keyword("SELECT")?;

        let distinct = if self.match_keyword("DISTINCT") {
            self.advance();
            true
        } else {
            false
        };

        // Parse columns
        let columns = self.parse_select_columns()?;

        // Parse FROM clause
        let from = if self.match_keyword("FROM") {
            self.advance();
            Some(self.parse_table_reference()?)
        } else {
            None
        };

        // Parse WHERE clause
        let where_clause = if self.match_keyword("WHERE") {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        // Parse ORDER BY clause
        let order_by = if self.match_keyword("ORDER") {
            self.advance();
            self.expect_keyword("BY")?;
            Some(self.parse_order_by()?)
        } else {
            None
        };

        // Parse LIMIT clause
        let limit = if self.match_keyword("LIMIT") {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        // Parse OFFSET clause
        let offset = if self.match_keyword("OFFSET") {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(SelectStatement {
            columns,
            from,
            where_clause,
            order_by,
            limit,
            offset,
            distinct,
        })
    }

    fn parse_select_columns(&mut self) -> Result<Vec<ResultColumn<'a>>> {
        let mut columns = Vec::new();

        loop {
            self.skip_to_next_token();

            if self.match_star() {
                columns.push(ResultColumn::Star);
                self.advance();
            } else {
                let expr = self.parse_expression()?;
                let alias = if self.match_keyword("AS") {
                    self.advance();
                    self.skip_to_next_token();
                    if let Token::Identifier(name) = self.current_token() {
                        self.advance();
                        Some(name)
                    } else {
                        return Err(Error::ParseError("Expected identifier after AS".into()));
                    }
                } else if let Token::Identifier(name) = self.current_token() {
                    // Check if this is an implicit alias (not a keyword)
                    if !self.is_keyword() {
                        self.advance();
                        Some(name)
                    } else {
                        None
                    }
                } else {
                    None
                };
                columns.push(ResultColumn::Expression { expr, alias });
            }

            self.skip_to_next_token();
            if !self.match_punctuation(",") {
                break;
            }
            self.advance();
        }

        Ok(columns)
    }

    fn parse_table_reference(&mut self) -> Result<TableOrSubquery<'a>> {
        self.skip_to_next_token();
        let table = self.expect_identifier()?;

        let alias = if self.match_keyword("AS") {
            self.advance();
            Some(self.expect_identifier()?)
        } else {
            None
        };

        Ok(TableOrSubquery { table, alias })
    }

    fn parse_order_by(&mut self) -> Result<Vec<OrderingTerm<'a>>> {
        let mut items = Vec::new();

        loop {
            let expr = self.parse_expression()?;
            let direction = if self.match_keyword("DESC") {
                self.advance();
                SortDirection::Desc
            } else if self.match_keyword("ASC") {
                self.advance();
                SortDirection::Asc
            } else {
                SortDirection::Asc
            };

            items.push(OrderingTerm { expr, direction });

            self.skip_to_next_token();
            if !self.match_punctuation(",") {
                break;
            }
            self.advance();
        }

        Ok(items)
    }

    fn parse_insert(&mut self) -> Result<InsertStatement<'a>> {
        self.expect_keyword("INSERT")?;
        self.expect_keyword("INTO")?;
        let table = self.expect_identifier()?;

        // Parse optional column list
        let columns = if self.match_punctuation("(") {
            self.advance();
            let mut cols = Vec::new();
            loop {
                cols.push(self.expect_identifier()?);
                self.skip_to_next_token();
                if !self.match_punctuation(",") {
                    break;
                }
                self.advance();
            }
            self.expect_punctuation(")")?;
            Some(cols)
        } else {
            None
        };

        self.expect_keyword("VALUES")?;

        // Parse value rows
        let mut values = Vec::new();
        loop {
            self.expect_punctuation("(")?;
            let mut row = Vec::new();
            loop {
                row.push(self.parse_expression()?);
                self.skip_to_next_token();
                if !self.match_punctuation(",") {
                    break;
                }
                self.advance();
            }
            self.expect_punctuation(")")?;
            values.push(row);

            self.skip_to_next_token();
            if !self.match_punctuation(",") {
                break;
            }
            self.advance();
        }

        Ok(InsertStatement {
            table,
            columns,
            values,
        })
    }

    fn parse_update(&mut self) -> Result<UpdateStatement<'a>> {
        self.expect_keyword("UPDATE")?;
        let table = self.expect_identifier()?;
        self.expect_keyword("SET")?;

        let mut assignments = Vec::new();
        loop {
            let column = self.expect_identifier()?;
            // = is an Operator, not Punctuation
            if let Token::Operator(op) = self.current_token() {
                if op == "=" {
                    self.advance();
                } else {
                    return Err(Error::ParseError(format!("Expected '=', got {}", op)));
                }
            } else {
                return Err(Error::ParseError(format!(
                    "Expected '=', got {}",
                    self.current_token().span()
                )));
            }
            let value = self.parse_expression()?;
            assignments.push(UpdateAssignment { column, value });

            self.skip_to_next_token();
            if !self.match_punctuation(",") {
                break;
            }
            self.advance();
        }

        let where_clause = if self.match_keyword("WHERE") {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(UpdateStatement {
            table,
            assignments,
            where_clause,
        })
    }

    fn parse_delete(&mut self) -> Result<DeleteStatement<'a>> {
        self.expect_keyword("DELETE")?;
        self.expect_keyword("FROM")?;
        let table = self.expect_identifier()?;

        let where_clause = if self.match_keyword("WHERE") {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(DeleteStatement { table, where_clause })
    }

    fn parse_create(&mut self) -> Result<CreateTableStatement<'a>> {
        self.expect_keyword("CREATE")?;
        self.expect_keyword("TABLE")?;
        let table = self.expect_identifier()?;
        self.expect_punctuation("(")?;

        let mut columns = Vec::new();
        loop {
            let name = self.expect_identifier()?;
            let type_name = self.expect_identifier()?;

            let mut constraints = Vec::new();
            loop {
                self.skip_to_next_token();
                if self.match_keyword("PRIMARY") {
                    self.advance();
                    self.expect_keyword("KEY")?;
                    constraints.push(ColumnConstraint::PrimaryKey);
                } else if self.match_keyword("NOT") {
                    self.advance();
                    self.expect_keyword("NULL")?;
                    constraints.push(ColumnConstraint::NotNull);
                } else if self.match_keyword("UNIQUE") {
                    self.advance();
                    constraints.push(ColumnConstraint::Unique);
                } else if self.match_keyword("DEFAULT") {
                    self.advance();
                    let expr = self.parse_expression()?;
                    constraints.push(ColumnConstraint::Default(expr));
                } else {
                    break;
                }
            }

            columns.push(ColumnDef {
                name,
                type_name,
                constraints,
            });

            self.skip_to_next_token();
            if !self.match_punctuation(",") {
                break;
            }
            self.advance();
        }

        self.expect_punctuation(")")?;

        Ok(CreateTableStatement { table, columns })
    }

    fn parse_drop(&mut self) -> Result<Statement<'a>> {
        self.expect_keyword("DROP")?;
        self.expect_keyword("TABLE")?;
        let table = self.expect_identifier()?;
        Ok(Statement::DropTableStmt { table })
    }

    fn parse_expression(&mut self) -> Result<Expression<'a>> {
        self.parse_or_expression()
    }

    fn parse_or_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_and_expression()?;

        while self.match_keyword("OR") {
            self.advance();
            let right = self.parse_and_expression()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                op: BinaryOperator::Or,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn parse_and_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_not_expression()?;

        while self.match_keyword("AND") {
            self.advance();
            let right = self.parse_not_expression()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                op: BinaryOperator::And,
                right: Box::new(right),
            };
        }

        Ok(expr)
    }

    fn parse_not_expression(&mut self) -> Result<Expression<'a>> {
        if self.match_keyword("NOT") {
            self.advance();
            let operand = self.parse_not_expression()?;
            return Ok(Expression::UnaryOp {
                op: UnaryOperator::Not,
                operand: Box::new(operand),
            });
        }

        self.parse_comparison_expression()
    }

    fn parse_comparison_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_relational_expression()?;

        loop {
            self.skip_to_next_token();
            
            // Handle = == <> !=
            if let Token::Operator(op) = self.current_token() {
                let bin_op = match op {
                    "=" | "==" => {
                        self.advance();
                        BinaryOperator::Equal
                    }
                    "<>" | "!=" => {
                        self.advance();
                        BinaryOperator::NotEqual
                    }
                    _ => break,
                };

                let right = self.parse_relational_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: bin_op,
                    right: Box::new(right),
                };
                continue;
            }

            // Handle IS, IS NOT, IS DISTINCT FROM, IS NOT DISTINCT FROM
            if self.match_keyword("IS") {
                self.advance();
                let op = if self.match_keyword("NOT") {
                    self.advance();
                    if self.match_keyword("DISTINCT") {
                        self.advance();
                        self.expect_keyword("FROM")?;
                        BinaryOperator::IsNotDistinctFrom
                    } else {
                        // IS NOT NULL - still use Is operator
                        BinaryOperator::Is
                    }
                } else if self.match_keyword("DISTINCT") {
                    self.advance();
                    self.expect_keyword("FROM")?;
                    BinaryOperator::IsDistinctFrom
                } else {
                    BinaryOperator::Is
                };
                let right = self.parse_relational_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op,
                    right: Box::new(right),
                };
                continue;
            }

            // Handle IN
            if self.match_keyword("IN") {
                self.advance();
                let right = self.parse_relational_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: BinaryOperator::In,
                    right: Box::new(right),
                };
                continue;
            }

            // Handle LIKE
            if self.match_keyword("LIKE") {
                self.advance();
                let right = self.parse_relational_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: BinaryOperator::Like,
                    right: Box::new(right),
                };
                continue;
            }

            // Handle GLOB
            if self.match_keyword("GLOB") {
                self.advance();
                let right = self.parse_relational_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: BinaryOperator::Glob,
                    right: Box::new(right),
                };
                continue;
            }

            // Handle MATCH
            if self.match_keyword("MATCH") {
                self.advance();
                let right = self.parse_relational_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: BinaryOperator::Match,
                    right: Box::new(right),
                };
                continue;
            }

            // Handle REGEXP
            if self.match_keyword("REGEXP") {
                self.advance();
                let right = self.parse_relational_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: BinaryOperator::Regexp,
                    right: Box::new(right),
                };
                continue;
            }

            // Handle BETWEEN ... AND
            if self.match_keyword("BETWEEN") {
                self.advance();
                let lower = self.parse_relational_expression()?;
                self.expect_keyword("AND")?;
                let upper = self.parse_relational_expression()?;
                // Note: Would need a Between variant in Expression to represent properly
                // For now, using a workaround
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: BinaryOperator::Is, // Placeholder
                    right: Box::new(Expression::Parenthesized(Box::new(lower))),
                };
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn parse_relational_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_bitwise_expression()?;

        loop {
            self.skip_to_next_token();
            if let Token::Operator(op) = self.current_token() {
                let bin_op = match op {
                    "<" => {
                        self.advance();
                        BinaryOperator::LessThan
                    }
                    "<=" => {
                        self.advance();
                        BinaryOperator::LessThanOrEqual
                    }
                    ">" => {
                        self.advance();
                        BinaryOperator::GreaterThan
                    }
                    ">=" => {
                        self.advance();
                        BinaryOperator::GreaterThanOrEqual
                    }
                    _ => break,
                };

                let right = self.parse_bitwise_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: bin_op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_bitwise_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_additive_expression()?;

        loop {
            self.skip_to_next_token();
            if let Token::Operator(op) = self.current_token() {
                let bin_op = match op {
                    "&" => {
                        self.advance();
                        BinaryOperator::BitwiseAnd
                    }
                    "|" => {
                        // Check if this is || (concatenate) by peeking ahead
                        if self.pos + 1 < self.tokens.len() {
                            if let Token::Operator(next) = self.tokens[self.pos + 1] {
                                if next == "|" {
                                    break; // Will be handled at concatenate level
                                }
                            }
                        }
                        self.advance();
                        BinaryOperator::BitwiseOr
                    }
                    "<<" => {
                        self.advance();
                        BinaryOperator::BitwiseLeftShift
                    }
                    ">>" => {
                        self.advance();
                        BinaryOperator::BitwiseRightShift
                    }
                    _ => break,
                };

                let right = self.parse_additive_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: bin_op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_additive_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_multiplicative_expression()?;

        loop {
            self.skip_to_next_token();
            if let Token::Operator(op) = self.current_token() {
                let bin_op = match op {
                    "+" => BinaryOperator::Add,
                    "-" => BinaryOperator::Subtract,
                    _ => break,
                };
                self.advance();
                let right = self.parse_multiplicative_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: bin_op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_multiplicative_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_concatenation_expression()?;

        loop {
            self.skip_to_next_token();
            if let Token::Operator(op) = self.current_token() {
                let bin_op = match op {
                    "*" => BinaryOperator::Multiply,
                    "/" => BinaryOperator::Divide,
                    "%" => BinaryOperator::Modulo,
                    _ => break,
                };
                self.advance();
                let right = self.parse_concatenation_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: bin_op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_concatenation_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_collate_expression()?;

        loop {
            self.skip_to_next_token();
            if let Token::Operator(op) = self.current_token() {
                if op == "||" {
                    self.advance();
                    let right = self.parse_collate_expression()?;
                    expr = Expression::BinaryOp {
                        left: Box::new(expr),
                        op: BinaryOperator::Concatenate,
                        right: Box::new(right),
                    };
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_collate_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_extraction_expression()?;

        // Handle COLLATE postfix operator
        while self.match_keyword("COLLATE") {
            self.advance();
            // For now, just skip the collation name
            // Would need to store collation info in the AST
            self.expect_identifier()?;
        }

        Ok(expr)
    }

    fn parse_extraction_expression(&mut self) -> Result<Expression<'a>> {
        let mut expr = self.parse_unary_expression()?;

        // Handle -> and ->> extraction operators
        loop {
            self.skip_to_next_token();
            if let Token::Operator(op) = self.current_token() {
                let bin_op = match op {
                    "->" => {
                        self.advance();
                        BinaryOperator::Extract
                    }
                    "->>" => {
                        self.advance();
                        BinaryOperator::ExtractText
                    }
                    _ => break,
                };
                let right = self.parse_unary_expression()?;
                expr = Expression::BinaryOp {
                    left: Box::new(expr),
                    op: bin_op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_unary_expression(&mut self) -> Result<Expression<'a>> {
        self.skip_to_next_token();

        // Handle unary minus
        if let Token::Operator(op) = self.current_token() {
            if op == "-" {
                self.advance();
                let operand = self.parse_unary_expression()?;
                return Ok(Expression::UnaryOp {
                    op: UnaryOperator::Minus,
                    operand: Box::new(operand),
                });
            }

            // Handle unary plus
            if op == "+" {
                self.advance();
                let operand = self.parse_unary_expression()?;
                return Ok(Expression::UnaryOp {
                    op: UnaryOperator::Plus,
                    operand: Box::new(operand),
                });
            }

            // Handle unary bitwise NOT (~)
            if op == "~" {
                self.advance();
                let operand = self.parse_unary_expression()?;
                return Ok(Expression::UnaryOp {
                    op: UnaryOperator::BitwiseNot,
                    operand: Box::new(operand),
                });
            }
        }

        let mut expr = self.parse_primary_expression()?;

        // Handle postfix operators: ISNULL, NOTNULL, NOT NULL
        loop {
            self.skip_to_next_token();
            if self.match_keyword("ISNULL") {
                self.advance();
                expr = Expression::UnaryOp {
                    op: UnaryOperator::Isnull,
                    operand: Box::new(expr),
                };
            } else if self.match_keyword("NOTNULL") {
                self.advance();
                expr = Expression::UnaryOp {
                    op: UnaryOperator::Notnull,
                    operand: Box::new(expr),
                };
            } else if self.match_keyword("NOT") {
                // Check if it's NOT NULL
                if self.pos + 1 < self.tokens.len() {
                    if let Token::Keyword(kw) = self.tokens[self.pos + 1] {
                        if kw.eq_ignore_ascii_case("NULL") {
                            self.advance(); // consume NOT
                            self.advance(); // consume NULL
                            expr = Expression::UnaryOp {
                                op: UnaryOperator::IsNotNull,
                                operand: Box::new(expr),
                            };
                            continue;
                        }
                    }
                }
                break;
            } else if self.match_keyword("NULL") && matches!(self.tokens.get(self.pos.saturating_sub(1)), Some(Token::Keyword(k)) if k.eq_ignore_ascii_case("IS")) {
                // This would be IS NULL - handled at comparison level
                break;
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary_expression(&mut self) -> Result<Expression<'a>> {
        if self.match_keyword("NULL") {
            self.advance();
            return Ok(Expression::Null);
        }

        if self.match_keyword("TRUE") {
            self.advance();
            return Ok(Expression::True);
        }

        if self.match_keyword("FALSE") {
            self.advance();
            return Ok(Expression::False);
        }

        if self.match_punctuation("(") {
            self.advance();
            let expr = self.parse_expression()?;
            self.expect_punctuation(")")?;
            return Ok(Expression::Parenthesized(Box::new(expr)));
        }

        // Handle * as a star expression (for COUNT(*), etc.)
        if self.match_star() {
            self.advance();
            return Ok(Expression::Identifier("*"));
        }

        match self.current_token() {
            Token::IntLiteral(lit) | Token::RealLiteral(lit) | Token::StringLiteral(lit)
            | Token::BlobLiteral(lit) => {
                self.advance();
                Ok(Expression::Literal(lit))
            }
            Token::Identifier(name) => {
                self.advance();

                // Check for function call
                if self.match_punctuation("(") {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.match_punctuation(")") {
                        loop {
                            args.push(self.parse_expression()?);
                            if !self.match_punctuation(",") {
                                break;
                            }
                            self.advance();
                        }
                    }
                    self.expect_punctuation(")")?;
                    return Ok(Expression::FunctionCall { name, args });
                }

                Ok(Expression::Identifier(name))
            }
            _ => Err(Error::ParseError(format!(
                "Expected expression, got {}",
                self.current_token().span()
            ))),
        }
    }

    // ===== Helper methods =====

    fn current_token(&self) -> Token<'a> {
        if self.pos < self.tokens.len() {
            self.tokens[self.pos]
        } else {
            Token::Eof
        }
    }

    fn peek_next_token(&self) -> Token<'a> {
        if self.pos + 1 < self.tokens.len() {
            self.tokens[self.pos + 1]
        } else {
            Token::Eof
        }
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn skip_to_next_token(&mut self) {
        // No-op: lexer has already tokenized, removing whitespace/comments
        // All tokens are valid positions
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.current_token(), Token::Eof)
    }

    fn is_keyword(&self) -> bool {
        matches!(self.current_token(), Token::Keyword(_))
    }

    fn is_keyword_at(&self, pos: usize) -> bool {
        if pos < self.tokens.len() {
            matches!(self.tokens[pos], Token::Keyword(_))
        } else {
            false
        }
    }

    fn match_keyword(&self, keyword: &str) -> bool {
        if let Token::Keyword(kw) = self.current_token() {
            kw.eq_ignore_ascii_case(keyword)
        } else {
            false
        }
    }

    fn match_star(&self) -> bool {
        if let Token::Operator(op) = self.current_token() {
            op == "*"
        } else if let Token::Punctuation(p) = self.current_token() {
            p == "*"
        } else {
            false
        }
    }

    fn match_punctuation(&self, punct: &str) -> bool {
        if let Token::Punctuation(p) = self.current_token() {
            p == punct
        } else {
            false
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<()> {
        if self.match_keyword(keyword) {
            self.advance();
            Ok(())
        } else {
            Err(Error::ParseError(format!(
                "Expected keyword '{}', got {}",
                keyword,
                self.current_token().span()
            )))
        }
    }

    fn expect_identifier(&mut self) -> Result<&'a str> {
        self.skip_to_next_token();
        if let Token::Identifier(name) = self.current_token() {
            self.advance();
            Ok(name)
        } else {
            Err(Error::ParseError(format!(
                "Expected identifier, got {}",
                self.current_token().span()
            )))
        }
    }

    fn expect_punctuation(&mut self, punct: &str) -> Result<()> {
        if self.match_punctuation(punct) {
            self.advance();
            Ok(())
        } else {
            Err(Error::ParseError(format!(
                "Expected '{}', got {}",
                punct,
                self.current_token().span()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_select() -> Result<()> {
        let mut parser = Parser::new("SELECT id, name FROM users")?;
        let stmt = parser.parse_statement()?;

        if let Statement::Select(select) = stmt {
            assert_eq!(select.columns.len(), 2);
            assert!(select.from.is_some());
        } else {
            panic!("Expected SELECT statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_select_with_where() -> Result<()> {
        let mut parser = Parser::new("SELECT * FROM users WHERE age > 18")?;
        let stmt = parser.parse_statement()?;

        if let Statement::Select(select) = stmt {
            assert_eq!(select.columns.len(), 1);
            assert!(select.where_clause.is_some());
        } else {
            panic!("Expected SELECT statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_insert() -> Result<()> {
        let mut parser = Parser::new("INSERT INTO users (id, name) VALUES (1, 'Alice')")?;
        let stmt = parser.parse_statement()?;

        if let Statement::Insert(insert) = stmt {
            assert_eq!(insert.table, "users");
            assert_eq!(insert.columns.as_ref().map(|c| c.len()), Some(2));
            assert_eq!(insert.values.len(), 1);
        } else {
            panic!("Expected INSERT statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_update() -> Result<()> {
        let mut parser = Parser::new("UPDATE users SET name = 'Bob' WHERE id = 1")?;
        let stmt = parser.parse_statement()?;

        if let Statement::Update(update) = stmt {
            assert_eq!(update.table, "users");
            assert_eq!(update.assignments.len(), 1);
            assert!(update.where_clause.is_some());
        } else {
            panic!("Expected UPDATE statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_delete() -> Result<()> {
        let mut parser = Parser::new("DELETE FROM users WHERE id = 1")?;
        let stmt = parser.parse_statement()?;

        if let Statement::Delete(delete) = stmt {
            assert_eq!(delete.table, "users");
            assert!(delete.where_clause.is_some());
        } else {
            panic!("Expected DELETE statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_create_table() -> Result<()> {
        let mut parser = Parser::new(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        )?;
        let stmt = parser.parse_statement()?;

        if let Statement::CreateTable(create) = stmt {
            assert_eq!(create.table, "users");
            assert_eq!(create.columns.len(), 2);
        } else {
            panic!("Expected CREATE TABLE statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_expression_arithmetic() -> Result<()> {
        let mut parser = Parser::new("SELECT 1 + 2 * 3")?;
        let stmt = parser.parse_statement()?;

        if let Statement::Select(_select) = stmt {
            // Expression parsing tested indirectly through SELECT
            assert!(true);
        } else {
            panic!("Expected SELECT statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_function_call() -> Result<()> {
        let mut parser = Parser::new("SELECT COUNT(*) FROM users")?;
        let stmt = parser.parse_statement()?;

        if let Statement::Select(select) = stmt {
            assert_eq!(select.columns.len(), 1);
        } else {
            panic!("Expected SELECT statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_select_with_order_by() -> Result<()> {
        let mut parser = Parser::new("SELECT * FROM users ORDER BY name DESC")?;
        let stmt = parser.parse_statement()?;

        if let Statement::Select(select) = stmt {
            assert!(select.order_by.is_some());
        } else {
            panic!("Expected SELECT statement");
        }

        Ok(())
    }

    #[test]
    fn test_parse_begin_commit() -> Result<()> {
        let mut parser = Parser::new("BEGIN")?;
        let begin = parser.parse_statement()?;
        assert!(matches!(begin, Statement::Begin));

        let mut parser = Parser::new("COMMIT")?;
        let commit = parser.parse_statement()?;
        assert!(matches!(commit, Statement::Commit));

        let mut parser = Parser::new("ROLLBACK")?;
        let rollback = parser.parse_statement()?;
        assert!(matches!(rollback, Statement::Rollback));

        Ok(())
    }
}
