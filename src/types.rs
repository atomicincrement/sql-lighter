//! SQL data types

use serde::{Deserialize, Serialize};

/// SQL data types supported by SQL Lighter
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlType {
    /// NULL type
    Null,
    
    /// Integer (8, 16, 32, or 64 bits)
    Integer,
    
    /// Real (floating point)
    Real,
    
    /// Text/String
    Text,
    
    /// Binary blob
    Blob,
}

/// SQL value - matches rusqlite::types::Value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    /// Get the type of this value
    pub fn sql_type(&self) -> SqlType {
        match self {
            Value::Null => SqlType::Null,
            Value::Integer(_) => SqlType::Integer,
            Value::Real(_) => SqlType::Real,
            Value::Text(_) => SqlType::Text,
            Value::Blob(_) => SqlType::Blob,
        }
    }
}

/// A row of data
pub type Row = Vec<(String, Value)>;
