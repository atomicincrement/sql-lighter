//! Parameter binding traits - rusqlite-compatible API (Phase 6b)
//!
//! References: https://docs.rs/rusqlite/latest/rusqlite/params.rs.html

use crate::types::Value;
use crate::error::Result;
use std::collections::HashMap;

/// Trait for types that can be converted to SQL values
///
/// This is similar to rusqlite::ToSql. Implementations allow values to be used
/// in parameterized queries.
pub trait ToSql {
    /// Convert self to a SQL Value
    fn to_sql(&self) -> Result<Value>;
}

/// Sealed trait to ensure Params impls are only in this crate
mod sealed {
    pub trait Sealed {}
}
use sealed::Sealed;

/// Trait for sets of parameters passed into SQL statements
///
/// This trait is similar to rusqlite::Params. It allows flexible ways to bind
/// parameters to queries, including tuples, arrays, and slices.
///
/// # Examples
///
/// Using a tuple:
/// ```ignore
/// conn.execute("INSERT INTO users VALUES (?1, ?2)", ("Alice", 30))?;
/// ```
///
/// Using an array:
/// ```ignore
/// conn.execute("SELECT * FROM users WHERE id IN (?1, ?2, ?3)", [1, 2, 3])?;
/// ```
///
/// Using a slice reference:
/// ```ignore
/// conn.execute("INSERT INTO users VALUES (?1, ?2)", &["Bob", "25"])?;
/// ```
pub trait Params: Sealed {
    /// Binds parameters to the connection by index (1-based)
    /// Returns a HashMap of parameter indices to their SQL values
    #[doc(hidden)]
    fn bind_params(self) -> Result<HashMap<String, Value>>;
}

// Implementations for empty tuple
impl Sealed for () {}
impl Params for () {
    fn bind_params(self) -> Result<HashMap<String, Value>> {
        Ok(HashMap::new())
    }
}

// Implementations for single-element tuple
impl<T: ToSql> Sealed for (T,) {}
impl<T: ToSql> Params for (T,) {
    fn bind_params(self) -> Result<HashMap<String, Value>> {
        let mut params = HashMap::new();
        params.insert("1".to_string(), self.0.to_sql()?);
        Ok(params)
    }
}

// Macro to generate tuple implementations for 2-16 elements
macro_rules! impl_tuple {
    ($($field:tt: $type:ident),+) => {
        impl<$($type: ToSql),+> Sealed for ($($type,)+) {}
        impl<$($type: ToSql),+> Params for ($($type,)+) {
            fn bind_params(self) -> Result<HashMap<String, Value>> {
                let mut params = HashMap::new();
                let mut index = 1;
                $(
                    params.insert(index.to_string(), self.$field.to_sql()?);
                    index += 1;
                )+
                Ok(params)
            }
        }
    };
}

impl_tuple!(0: A, 1: B);
impl_tuple!(0: A, 1: B, 2: C);
impl_tuple!(0: A, 1: B, 2: C, 3: D);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L, 12: M);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L, 12: M, 13: N);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L, 12: M, 13: N, 14: O);
impl_tuple!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L, 12: M, 13: N, 14: O, 15: P);

// Array implementations for sizes 1-32
macro_rules! impl_array {
    ($($N:literal)+) => {$(
        impl<T: ToSql> Sealed for [T; $N] {}
        impl<T: ToSql> Params for [T; $N] {
            fn bind_params(self) -> Result<HashMap<String, Value>> {
                let mut params = HashMap::new();
                for (i, item) in self.iter().enumerate() {
                    params.insert((i + 1).to_string(), item.to_sql()?);
                }
                Ok(params)
            }
        }

        impl<T: ToSql + ?Sized> Sealed for &[&T; $N] {}
        impl<T: ToSql + ?Sized> Params for &[&T; $N] {
            fn bind_params(self) -> Result<HashMap<String, Value>> {
                let mut params = HashMap::new();
                for (i, item) in self.iter().enumerate() {
                    params.insert((i + 1).to_string(), item.to_sql()?);
                }
                Ok(params)
            }
        }
    )+};
}

impl_array!(
    1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17
    18 19 20 21 22 23 24 25 26 27 28 29 30 31 32
);

// Slice of references implementation
impl<T: ToSql + ?Sized> Sealed for &[&T] {}
impl<T: ToSql + ?Sized> Params for &[&T] {
    fn bind_params(self) -> Result<HashMap<String, Value>> {
        let mut params = HashMap::new();
        for (i, item) in self.iter().enumerate() {
            params.insert((i + 1).to_string(), item.to_sql()?);
        }
        Ok(params)
    }
}

// ToSql implementations for common types
impl ToSql for Value {
    fn to_sql(&self) -> Result<Value> {
        Ok(self.clone())
    }
}

impl ToSql for i32 {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Integer(*self as i64))
    }
}

impl ToSql for i64 {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Integer(*self))
    }
}

impl ToSql for u32 {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Integer(*self as i64))
    }
}

impl ToSql for f64 {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Real(*self))
    }
}

impl ToSql for f32 {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Real(*self as f64))
    }
}

impl ToSql for bool {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Integer(if *self { 1 } else { 0 }))
    }
}

impl ToSql for String {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Text(self.clone()))
    }
}

impl ToSql for str {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Text(self.to_string()))
    }
}

impl<'a> ToSql for &'a str {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Text(self.to_string()))
    }
}

// Reference implementations for primitive types
impl ToSql for &i32 {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Integer(**self as i64))
    }
}

impl ToSql for &i64 {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Integer(**self))
    }
}

impl ToSql for &f64 {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Real(**self))
    }
}

impl ToSql for &bool {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Integer(if **self { 1 } else { 0 }))
    }
}

impl<T: ToSql> ToSql for Option<T> {
    fn to_sql(&self) -> Result<Value> {
        match self {
            Some(val) => val.to_sql(),
            None => Ok(Value::Null),
        }
    }
}

impl ToSql for [u8] {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Blob(self.to_vec()))
    }
}

impl ToSql for Vec<u8> {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Blob(self.clone()))
    }
}

impl ToSql for &String {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Text((*self).clone()))
    }
}

impl ToSql for &Vec<u8> {
    fn to_sql(&self) -> Result<Value> {
        Ok(Value::Blob((*self).clone()))
    }
}

impl<T: ToSql> ToSql for &Option<T> {
    fn to_sql(&self) -> Result<Value> {
        match self {
            Some(val) => val.to_sql(),
            None => Ok(Value::Null),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tuple_params() {
        let params: () = ();
        let result = params.bind_params().unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_single_element_tuple() {
        let params = ("hello",);
        let result = params.bind_params().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("1").unwrap(), &Value::Text("hello".to_string()));
    }

    #[test]
    fn test_multi_element_tuple() {
        let params = (42i32, "world", 3.14f64);
        let result = params.bind_params().unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("1").unwrap(), &Value::Integer(42));
        assert_eq!(result.get("2").unwrap(), &Value::Text("world".to_string()));
        assert_eq!(result.get("3").unwrap(), &Value::Real(3.14));
    }

    #[test]
    fn test_array_params() {
        let params = [1i32, 2, 3];
        let result = params.bind_params().unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("1").unwrap(), &Value::Integer(1));
        assert_eq!(result.get("2").unwrap(), &Value::Integer(2));
        assert_eq!(result.get("3").unwrap(), &Value::Integer(3));
    }

    #[test]
    fn test_slice_params() {
        let strings = ["foo", "bar", "baz"];
        let params: &[&str] = &strings;
        let result = params.bind_params().unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("1").unwrap(), &Value::Text("foo".to_string()));
        assert_eq!(result.get("2").unwrap(), &Value::Text("bar".to_string()));
        assert_eq!(result.get("3").unwrap(), &Value::Text("baz".to_string()));
    }

    #[test]
    fn test_tosql_option() {
        let some_val: Option<i32> = Some(42);
        let result = some_val.to_sql().unwrap();
        assert_eq!(result, Value::Integer(42));

        let none_val: Option<i32> = None;
        let result = none_val.to_sql().unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_tosql_bool() {
        let true_val = true;
        let result = true_val.to_sql().unwrap();
        assert_eq!(result, Value::Integer(1));

        let false_val = false;
        let result = false_val.to_sql().unwrap();
        assert_eq!(result, Value::Integer(0));
    }
}
