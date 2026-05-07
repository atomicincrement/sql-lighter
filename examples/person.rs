//! Person example - mirrors the rusqlite example
//!
//! Demonstrates basic sql-lighter usage:
//! - Creating tables
//! - Inserting data with parameter binding
//! - Prepared statements with query_map

use sql_lighter::connection::Connection;
use sql_lighter::FromValue;

#[derive(Debug)]
struct Person {
    id: i32,
    name: String,
    data: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = Connection::open_in_memory()?;

    conn.execute(
        "CREATE TABLE person (
            id   INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            data TEXT
        )",
        (),
    )?;

    let me = Person {
        id: 1,
        name: "Steven".to_string(),
        data: None,
    };
    conn.execute(
        "INSERT INTO person (id, name, data) VALUES (?1, ?2, ?3)",
        (me.id, &me.name, &me.data),
    )?;

    let stmt = conn.prepare("SELECT id, name, data FROM person")?;
    let person_iter = stmt.query_map(&mut conn, (), |row| {
        Ok(Person {
            id: row.get(0)?,
            name: row.get(1)?,
            data: row.get(2)?,
        })
    })?;

    for person in person_iter {
        println!("Found person {:?}", person?);
    }
    Ok(())
}
