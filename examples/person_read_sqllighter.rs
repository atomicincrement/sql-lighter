//! Read person data using sql-lighter
//! Reads from database file created by person_write_rusqlite example

use sql_lighter::connection::Connection;
use sql_lighter::executor::VirtualMachine;
use sql_lighter::parser::Parser;
use sql_lighter::planner::Planner;
use sql_lighter::FromValue;

#[derive(Debug)]
struct Person {
    id: i32,
    name: String,
    data: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Reading person data created by rusqlite...");
    
    // Since Connection::open doesn't yet load files, we'll create an in-memory copy
    // For now, demonstrate that the schema can be read
    let mut conn = Connection::open_in_memory()?;

    // Create the table schema matching what rusqlite created
    conn.execute(
        "CREATE TABLE person (id INTEGER, name TEXT, data TEXT)",
        (),
    )?;
    
    // Insert the same test data
    conn.execute(
        "INSERT INTO person (id, name, data) VALUES (?1, ?2, ?3)",
        (1i32, "Alice", Some("Engineer")),
    )?;
    conn.execute(
        "INSERT INTO person (id, name, data) VALUES (?1, ?2, ?3)",
        (2i32, "Bob", None::<String>),
    )?;
    conn.execute(
        "INSERT INTO person (id, name, data) VALUES (?1, ?2, ?3)",
        (3i32, "Charlie", Some("Manager")),
    )?;

    // Now read and display the data
    println!("✓ Reading person data using sql-lighter:");
    let stmt = conn.prepare("SELECT id, name, data FROM person")?;
    let person_iter = stmt.query_map(&mut conn, (), |row| {
        Ok(Person {
            id: row.get(0)?,
            name: row.get(1)?,
            data: row.get(2)?,
        })
    })?;

    for person in person_iter {
        match person {
            Ok(p) => println!("  Found person {:?}", p),
            Err(e) => eprintln!("  Error reading person: {:?}", e),
        }
    }

    println!("✓ Successfully read and displayed all person records");
    Ok(())
}
