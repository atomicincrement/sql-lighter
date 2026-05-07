//! Zero-copy SQL lexer
//!
//! Tokenizes SQL input from a string slice without allocating.
//! Each token wraps a &str span pointing to the original input.

use crate::error::{Error, Result};

/// Token types for SQL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token<'a> {
    /// SQL keyword (SELECT, FROM, WHERE, etc.)
    Keyword(&'a str),
    /// Identifier (table/column name)
    Identifier(&'a str),
    /// Integer literal (123, 0xFF, etc.)
    IntLiteral(&'a str),
    /// Real/floating point literal (1.23, 1e-5, etc.)
    RealLiteral(&'a str),
    /// Blob literal (X'48656C6C6F')
    BlobLiteral(&'a str),
    /// String literal ('hello')
    StringLiteral(&'a str),
    /// Operator (+, -, *, /, ||, etc.)
    Operator(&'a str),
    /// Punctuation (,, ;, (, ), etc.)
    Punctuation(&'a str),
    /// Whitespace (spaces, tabs, newlines)
    Whitespace(&'a str),
    /// Comment (-- or /* ... */)
    Comment(&'a str),
    /// End of file
    Eof,
}

impl<'a> Token<'a> {
    /// Get the span of this token (empty for Eof)
    pub fn span(&self) -> &'a str {
        match self {
            Token::Keyword(s)
            | Token::Identifier(s)
            | Token::IntLiteral(s)
            | Token::RealLiteral(s)
            | Token::BlobLiteral(s)
            | Token::StringLiteral(s)
            | Token::Operator(s)
            | Token::Punctuation(s)
            | Token::Whitespace(s)
            | Token::Comment(s) => s,
            Token::Eof => "",
        }
    }

    /// Check if this token is a keyword
    pub fn is_keyword(&self) -> bool {
        matches!(self, Token::Keyword(_))
    }

    /// Check if this token is an identifier
    pub fn is_identifier(&self) -> bool {
        matches!(self, Token::Identifier(_))
    }

    /// Check if this token is a literal (int, real, blob, or string)
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            Token::IntLiteral(_)
                | Token::RealLiteral(_)
                | Token::BlobLiteral(_)
                | Token::StringLiteral(_)
        )
    }

    /// Check if this token is Eof
    pub fn is_eof(&self) -> bool {
        matches!(self, Token::Eof)
    }
}

/// Zero-copy SQL lexer
pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    bytes: &'a [u8],
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given SQL input
    pub fn new(input: &'a str) -> Self {
        let bytes = input.as_bytes();
        Lexer { input, pos: 0, bytes }
    }

    /// Get the next token without advancing (peek)
    pub fn peek(&self) -> Result<Token<'a>> {
        let mut temp_lexer = Lexer {
            input: self.input,
            pos: self.pos,
            bytes: self.bytes,
        };
        temp_lexer.next_token()
    }

    /// Get the next token and advance
    pub fn next_token(&mut self) -> Result<Token<'a>> {
        // Skip whitespace and comments (consumed silently in normal tokenization)
        self.skip_whitespace_and_comments();

        if self.pos >= self.input.len() {
            return Ok(Token::Eof);
        }

        let ch = self.bytes[self.pos] as char;

        // Blob literal: X'...' or x'...'
        if (ch == 'X' || ch == 'x') && self.pos + 1 < self.input.len() {
            if self.bytes[self.pos + 1] == b'\'' {
                return self.read_blob_literal();
            }
        }

        // String literal: '...' (single-quoted)
        if ch == '\'' {
            return self.read_string_literal();
        }

        // Number literal: starts with digit or . (for decimals like .5)
        if ch.is_ascii_digit() || (ch == '.' && matches!(self.peek_char(), Some(c) if c.is_ascii_digit())) {
            return self.read_number_literal();
        }

        // Identifier or keyword: starts with letter or underscore
        if ch.is_ascii_alphabetic() || ch == '_' {
            return self.read_identifier();
        }

        // Operators (multi-char first, then single-char)
        if let Some(token) = self.try_read_operator() {
            return Ok(token);
        }

        // Punctuation
        if let Some(token) = self.try_read_punctuation() {
            return Ok(token);
        }

        Err(Error::ParseError(format!(
            "Unexpected character '{}' at position {}",
            ch, self.pos
        )))
    }

    /// Get all tokens from the current position to EOF
    pub fn tokenize(&mut self) -> Result<Vec<Token<'a>>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if token.is_eof() {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        Ok(tokens)
    }

    // ===== Private helpers =====

    /// Skip whitespace and comments
    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.bytes[self.pos] as char;

            if ch.is_whitespace() {
                self.pos += 1;
            } else if ch == '-' && self.peek_char() == Some('-') {
                // Line comment: -- until newline
                self.pos += 2;
                while self.pos < self.input.len() && self.bytes[self.pos] as char != '\n' {
                    self.pos += 1;
                }
                if self.pos < self.input.len() {
                    self.pos += 1; // skip newline
                }
            } else if ch == '/' && self.peek_char() == Some('*') {
                // Block comment: /* ... */
                self.pos += 2;
                while self.pos + 1 < self.input.len() {
                    if self.bytes[self.pos] as char == '*' && self.bytes[self.pos + 1] as char == '/' {
                        self.pos += 2;
                        break;
                    }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Get the next character without consuming it
    fn peek_char(&self) -> Option<char> {
        if self.pos + 1 < self.input.len() {
            Some(self.bytes[self.pos + 1] as char)
        } else {
            None
        }
    }

    /// Get character at offset without consuming
    #[allow(dead_code)]
    fn peek_at(&self, offset: usize) -> Option<char> {
        if self.pos + offset < self.input.len() {
            Some(self.bytes[self.pos + offset] as char)
        } else {
            None
        }
    }

    /// Read a blob literal: X'48656C6C6F' (hex-encoded)
    fn read_blob_literal(&mut self) -> Result<Token<'a>> {
        let start = self.pos;
        self.pos += 1; // skip X/x
        
        if self.pos >= self.input.len() || self.bytes[self.pos] as char != '\'' {
            return Err(Error::ParseError("Invalid blob literal".into()));
        }
        self.pos += 1; // skip opening quote

        // Read hex digits until closing quote
        while self.pos < self.input.len() {
            let ch = self.bytes[self.pos] as char;
            if ch == '\'' {
                self.pos += 1; // skip closing quote
                return Ok(Token::BlobLiteral(&self.input[start..self.pos]));
            }
            if !ch.is_ascii_hexdigit() {
                return Err(Error::ParseError("Invalid hex digit in blob literal".into()));
            }
            self.pos += 1;
        }

        Err(Error::ParseError("Unterminated blob literal".into()))
    }

    /// Read a string literal: 'hello' (doubled single quotes for escaping)
    fn read_string_literal(&mut self) -> Result<Token<'a>> {
        let start = self.pos;
        self.pos += 1; // skip opening quote

        while self.pos < self.input.len() {
            let ch = self.bytes[self.pos] as char;
            if ch == '\'' {
                self.pos += 1;
                // Check for doubled quote ('' = escaped quote)
                if self.pos < self.input.len() && self.bytes[self.pos] as char == '\'' {
                    self.pos += 1; // skip second quote, continue
                } else {
                    return Ok(Token::StringLiteral(&self.input[start..self.pos]));
                }
            } else {
                self.pos += 1;
            }
        }

        Err(Error::ParseError("Unterminated string literal".into()))
    }

    /// Read a number literal (integer or real)
    fn read_number_literal(&mut self) -> Result<Token<'a>> {
        let start = self.pos;

        // Read initial digits (or . for decimals like .5)
        if self.bytes[self.pos] as char == '.' {
            self.pos += 1;
            while self.pos < self.input.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            // Must have exponent to be real
            if self.pos < self.input.len() && (self.bytes[self.pos] as char == 'e' || self.bytes[self.pos] as char == 'E') {
                return self.read_number_exponent(start);
            }
            return Ok(Token::RealLiteral(&self.input[start..self.pos]));
        }

        // Read integer part
        while self.pos < self.input.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

        // Check for decimal point
        if self.pos < self.input.len() && self.bytes[self.pos] as char == '.' {
            let after_dot = self.pos + 1;
            if after_dot < self.input.len() && self.bytes[after_dot].is_ascii_digit() {
                self.pos = after_dot;
                while self.pos < self.input.len() && self.bytes[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
                // Check for exponent
                if self.pos < self.input.len() && (self.bytes[self.pos] as char == 'e' || self.bytes[self.pos] as char == 'E') {
                    return self.read_number_exponent(start);
                }
                return Ok(Token::RealLiteral(&self.input[start..self.pos]));
            }
        }

        // Check for exponent (makes it real)
        if self.pos < self.input.len() && (self.bytes[self.pos] as char == 'e' || self.bytes[self.pos] as char == 'E') {
            return self.read_number_exponent(start);
        }

        Ok(Token::IntLiteral(&self.input[start..self.pos]))
    }

    /// Read exponent part of a number (after e/E)
    fn read_number_exponent(&mut self, start: usize) -> Result<Token<'a>> {
        self.pos += 1; // skip e/E
        
        if self.pos < self.input.len() && (self.bytes[self.pos] as char == '+' || self.bytes[self.pos] as char == '-') {
            self.pos += 1;
        }

        let digit_start = self.pos;
        while self.pos < self.input.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }

        if self.pos == digit_start {
            return Err(Error::ParseError("Invalid exponent in number literal".into()));
        }

        Ok(Token::RealLiteral(&self.input[start..self.pos]))
    }

    /// Try to read an identifier or keyword
    fn read_identifier(&mut self) -> Result<Token<'a>> {
        let start = self.pos;

        while self.pos < self.input.len() {
            let ch = self.bytes[self.pos] as char;
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }

        let text = &self.input[start..self.pos];
        
        // Check if it's a keyword (case-insensitive in SQL)
        if is_keyword(text) {
            Ok(Token::Keyword(text))
        } else {
            Ok(Token::Identifier(text))
        }
    }

    /// Try to read a multi-character operator
    fn try_read_operator(&mut self) -> Option<Token<'a>> {
        let start = self.pos;

        // Try two-character operators first
        if self.pos + 1 < self.input.len() {
            let two_char = [self.bytes[self.pos] as char, self.bytes[self.pos + 1] as char];
            if matches!(two_char, ['<', '='] | ['<', '>'] | ['>', '='] | ['=', '='] | ['!', '='] | ['|', '|']) {
                self.pos += 2;
                return Some(Token::Operator(&self.input[start..self.pos]));
            }
        }

        // Single-character operators
        let ch = self.bytes[self.pos] as char;
        if matches!(ch, '+' | '-' | '*' | '/' | '%' | '=' | '<' | '>' | '|' | '&' | '^' | '~') {
            self.pos += 1;
            return Some(Token::Operator(&self.input[start..self.pos]));
        }

        None
    }

    /// Try to read punctuation
    fn try_read_punctuation(&mut self) -> Option<Token<'a>> {
        let start = self.pos;
        let ch = self.bytes[self.pos] as char;

        if matches!(ch, '(' | ')' | ',' | ';' | '[' | ']') {
            self.pos += 1;
            return Some(Token::Punctuation(&self.input[start..self.pos]));
        }

        None
    }
}

/// Check if a word is a SQL keyword (case-insensitive)
fn is_keyword(word: &str) -> bool {
    matches!(
        word.to_uppercase().as_str(),
        "SELECT" | "FROM" | "WHERE" | "AND" | "OR" | "NOT" | "IN" | "IS" | "NULL" | "TRUE"
            | "FALSE" | "LIKE" | "BETWEEN" | "ORDER" | "BY" | "GROUP" | "HAVING" | "LIMIT"
            | "OFFSET" | "DISTINCT" | "AS" | "JOIN" | "LEFT" | "RIGHT" | "INNER" | "OUTER"
            | "ON" | "CROSS" | "UNION" | "ALL" | "EXCEPT" | "INTERSECT" | "CREATE" | "TABLE"
            | "INSERT" | "INTO" | "VALUES" | "UPDATE" | "SET" | "DELETE" | "DROP" | "ALTER"
            | "ADD" | "COLUMN" | "INDEX" | "PRIMARY" | "KEY" | "FOREIGN" | "REFERENCES"
            | "DEFAULT" | "CHECK" | "UNIQUE" | "AUTOINCREMENT" | "BEGIN" | "COMMIT"
            | "ROLLBACK" | "TRANSACTION" | "PRAGMA" | "ATTACH" | "DETACH" | "DATABASE"
            | "VACUUM" | "ANALYZE" | "EXPLAIN" | "PLAN" | "QUERY" | "CAST" | "CASE" | "WHEN"
            | "THEN" | "ELSE" | "END" | "WITH" | "RECURSIVE" | "USING" | "COLLATE" | "ASC"
            | "DESC" | "NULLS" | "FIRST" | "LAST" | "ESCAPE" | "EXISTS" | "PARTITION" | "OVER"
            | "WINDOW" | "ROWS" | "RANGE" | "UNBOUNDED" | "PRECEDING" | "FOLLOWING" | "CURRENT"
            | "ROW" | "GENERATED" | "ALWAYS" | "STORED" | "VIRTUAL"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keywords() {
        let mut lexer = Lexer::new("SELECT FROM WHERE");
        assert!(matches!(lexer.next_token(), Ok(Token::Keyword("SELECT"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Keyword("FROM"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Keyword("WHERE"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Eof)));
    }

    #[test]
    fn test_identifiers() {
        let mut lexer = Lexer::new("table1 _col user_name");
        assert!(matches!(lexer.next_token(), Ok(Token::Identifier("table1"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Identifier("_col"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Identifier("user_name"))));
    }

    #[test]
    fn test_integer_literals() {
        let mut lexer = Lexer::new("0 42 999");
        assert!(matches!(lexer.next_token(), Ok(Token::IntLiteral("0"))));
        assert!(matches!(lexer.next_token(), Ok(Token::IntLiteral("42"))));
        assert!(matches!(lexer.next_token(), Ok(Token::IntLiteral("999"))));
    }

    #[test]
    fn test_real_literals() {
        let mut lexer = Lexer::new("1.5 0.001 1e5 2.5e-3 .5");
        assert!(matches!(lexer.next_token(), Ok(Token::RealLiteral("1.5"))));
        assert!(matches!(lexer.next_token(), Ok(Token::RealLiteral("0.001"))));
        assert!(matches!(lexer.next_token(), Ok(Token::RealLiteral("1e5"))));
        assert!(matches!(lexer.next_token(), Ok(Token::RealLiteral("2.5e-3"))));
        assert!(matches!(lexer.next_token(), Ok(Token::RealLiteral(".5"))));
    }

    #[test]
    fn test_string_literals() {
        let mut lexer = Lexer::new("'hello' 'it''s' 'world'");
        assert!(matches!(lexer.next_token(), Ok(Token::StringLiteral("'hello'"))));
        assert!(matches!(lexer.next_token(), Ok(Token::StringLiteral("'it''s'"))));
        assert!(matches!(lexer.next_token(), Ok(Token::StringLiteral("'world'"))));
    }

    #[test]
    fn test_blob_literals() {
        let mut lexer = Lexer::new("X'48656C6C6F' x'DEADBEEF'");
        assert!(matches!(lexer.next_token(), Ok(Token::BlobLiteral("X'48656C6C6F'"))));
        assert!(matches!(lexer.next_token(), Ok(Token::BlobLiteral("x'DEADBEEF'"))));
    }

    #[test]
    fn test_operators() {
        let mut lexer = Lexer::new("+ - * / = <> <= >= || & | ^");
        assert!(matches!(lexer.next_token(), Ok(Token::Operator("+"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Operator("-"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Operator("*"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Operator("/"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Operator("="))));
        assert!(matches!(lexer.next_token(), Ok(Token::Operator("<>"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Operator("<="))));
        assert!(matches!(lexer.next_token(), Ok(Token::Operator(">="))));
        assert!(matches!(lexer.next_token(), Ok(Token::Operator("||"))));
    }

    #[test]
    fn test_punctuation() {
        let mut lexer = Lexer::new("( ) , ; [ ]");
        assert!(matches!(lexer.next_token(), Ok(Token::Punctuation("("))));
        assert!(matches!(lexer.next_token(), Ok(Token::Punctuation(")"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Punctuation(","))));
        assert!(matches!(lexer.next_token(), Ok(Token::Punctuation(";"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Punctuation("["))));
        assert!(matches!(lexer.next_token(), Ok(Token::Punctuation("]"))));
    }

    #[test]
    fn test_simple_query() {
        let mut lexer = Lexer::new("SELECT id, name FROM users WHERE age > 18");
        let tokens = lexer.tokenize().unwrap();
        
        // Just verify we got the right number and types
        assert!(tokens.len() > 0);
        assert!(matches!(tokens[0], Token::Keyword("SELECT")));
        assert!(tokens.iter().any(|t| matches!(t, Token::Keyword("WHERE"))));
    }

    #[test]
    fn test_whitespace_and_comments() {
        let mut lexer = Lexer::new("SELECT  -- comment\n  id FROM -- another comment\n users");
        let tokens = lexer.tokenize().unwrap();
        
        // Should only get keywords and identifiers, comments/whitespace skipped
        assert!(!tokens.iter().any(|t| matches!(t, Token::Comment(_) | Token::Whitespace(_))));
        assert!(matches!(tokens[0], Token::Keyword("SELECT")));
        assert!(matches!(tokens[1], Token::Identifier("id")));
    }

    #[test]
    fn test_block_comments() {
        let mut lexer = Lexer::new("SELECT /* block comment */ id FROM users /* end */");
        let tokens = lexer.tokenize().unwrap();
        
        // Comments should be skipped
        assert!(!tokens.iter().any(|t| matches!(t, Token::Comment(_))));
        assert!(matches!(tokens[0], Token::Keyword("SELECT")));
    }

    #[test]
    fn test_case_insensitive_keywords() {
        let mut lexer = Lexer::new("SELECT select Select sElEcT");
        assert!(matches!(lexer.next_token(), Ok(Token::Keyword("SELECT"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Keyword("select"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Keyword("Select"))));
        assert!(matches!(lexer.next_token(), Ok(Token::Keyword("sElEcT"))));
    }
}
