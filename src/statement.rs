//! Prepared statements - rusqlite-compatible API (Phase 6c)
//!
//! Provides prepared statement support with query_map for row mapping.
//! References: https://github.com/rusqlite/rusqlite

use crate::error::{Error, Result};
use crate::connection::Connection;
use crate::params::Params;
use crate::types::Value;

/// A prepared SQL statement
#[derive(Debug, Clone)]
pub struct Statement {
    /// The SQL query string
    sql: String,
}

impl Statement {
    /// Create a new prepared statement
    pub fn new(sql: String) -> Self {
        Statement { sql }
    }

    /// Execute a query and map rows using a closure
    ///
    /// # Arguments
    /// * `conn` - Connection to use for execution
    /// * `params` - Parameters for the query
    /// * `f` - Closure that maps each row to a result type
    ///
    /// # Example
    /// ```ignore
    /// let stmt = conn.prepare("SELECT id, name FROM users WHERE id = ?1")?;
    /// let results = stmt.query_map(conn, (42,), |row| {
    ///     Ok((row.get(0)?, row.get(1)?))
    /// })?;
    /// ```
    pub fn query_map<P, F, T>(
        &self,
        conn: &mut Connection,
        params: P,
        f: F,
    ) -> Result<Vec<Result<T>>>
    where
        P: Params,
        F: Fn(RowRef) -> Result<T>,
        T: 'static,
    {
        // Execute the query
        let result = conn.execute(&self.sql, params)?;
        
        // Map each row using the closure
        let mapped: Vec<Result<T>> = result
            .rows
            .into_iter()
            .map(|row| f(RowRef { row }))
            .collect();
        
        Ok(mapped)
    }
}

/// A reference to a row that provides column access like rusqlite
#[derive(Debug, Clone)]
pub struct RowRef {
    /// The underlying row data
    row: crate::types::Row,
}

impl RowRef {
    /// Get a value from the row by column index
    ///
    /// # Arguments
    /// * `index` - 0-based column index
    ///
    /// # Example
    /// ```ignore
    /// let value: i32 = row.get(0)?;
    /// let text: String = row.get(1)?;
    /// ```
    pub fn get<T: FromValue>(&self, index: usize) -> Result<T> {
        self.row
            .get(index)
            .map(|(_, value)| value)
            .ok_or(Error::InvalidColumnIndex(index))
            .and_then(|value| T::from_value(value))
    }
}

/// Trait for converting SQL values to Rust types
pub trait FromValue: Sized {
    /// Convert a SQL value to this Rust type
    fn from_value(value: &Value) -> Result<Self>;
}

impl FromValue for i32 {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Integer(i) => Ok(*i as i32),
            _ => Err(Error::InvalidColumnType(
                0,
                format!("{:?}", value.sql_type()),
                "i32".to_string(),
            )),
        }
    }
}

impl FromValue for i64 {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Integer(i) => Ok(*i),
            _ => Err(Error::InvalidColumnType(
                0,
                format!("{:?}", value.sql_type()),
                "i64".to_string(),
            )),
        }
    }
}

impl FromValue for f64 {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Real(f) => Ok(*f),
            _ => Err(Error::InvalidColumnType(
                0,
                format!("{:?}", value.sql_type()),
                "f64".to_string(),
            )),
        }
    }
}

impl FromValue for String {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Text(s) => Ok(s.clone()),
            _ => Err(Error::InvalidColumnType(
                0,
                format!("{:?}", value.sql_type()),
                "String".to_string(),
            )),
        }
    }
}

impl FromValue for Vec<u8> {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Blob(b) => Ok(b.clone()),
            _ => Err(Error::InvalidColumnType(
                0,
                format!("{:?}", value.sql_type()),
                "Vec<u8>".to_string(),
            )),
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: &Value) -> Result<Self> {
        match value {
            Value::Null => Ok(None),
            _ => T::from_value(value).map(Some),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Connection;

    #[test]
    fn test_from_value_conversions() -> Result<()> {
        let int_val = Value::Integer(42);
        assert_eq!(i32::from_value(&int_val)?, 42i32);

        let text_val = Value::Text("hello".to_string());
        assert_eq!(String::from_value(&text_val)?, "hello");

        let null_val = Value::Null;
        let opt: Option<String> = Option::from_value(&null_val)?;
        assert!(opt.is_none());

        let some_val = Value::Text("world".to_string());
        let opt: Option<String> = Option::from_value(&some_val)?;
        assert_eq!(opt, Some("world".to_string()));

        Ok(())
    }
}
