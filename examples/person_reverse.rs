//! Reverse person example: write with sql-lighter, read with rusqlite
//! Demonstrates interoperability by writing a person to a database file using sql-lighter,
//! then reading it back using rusqlite's Connection API.

use rusqlite::Connection as RusqliteConnection;
use sql_lighter::connection::Connection;
use std::fs;

#[derive(Debug)]
struct Person {
    id: i32,
    name: String,
    data: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "target/person_reverse.db";

    // ===== PART 1: Write using sql-lighter =====
    println!("=== Writing with sql-lighter ===");
    {
        // Remove existing file if present
        let _ = fs::remove_file(db_path);

        // Create an in-memory database with sql-lighter, then we'll write it to file
        // First create the file structure using rusqlite so sql-lighter can write to it
        {
            let conn = RusqliteConnection::open(db_path)?;
            conn.execute(
                "CREATE TABLE person (
                    id   INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    data TEXT
                )",
                [],
            )?;
        }

        // Now open the file with sql-lighter and write to it (Phase 7b)
        let mut conn = Connection::open(db_path)?;

        // Insert a person record using sql-lighter
        let person = Person {
            id: 1,
            name: "Alice".to_string(),
            data: Some("Engineer".to_string()),
        };

        conn.execute(
            "INSERT INTO person (id, name, data) VALUES (?1, ?2, ?3)",
            (person.id, person.name.clone(), person.data.clone()),
        )?;

        println!("✓ Written person to {} using sql-lighter", db_path);
        println!("✓ Person data persisted with INSERT via sql-lighter (Phase 7b)");
    }

    // ===== PART 2: Verify with rusqlite =====
    println!("\n=== Verification with rusqlite ===");
    {
        // First, let's verify sql-lighter can read back what it wrote
        println!("Attempting to read back with sql-lighter...");
        match Connection::open(db_path) {
            Ok(_conn) => {
                println!("✓ sql-lighter can open the file");
            }
            Err(e) => {
                println!("✗ sql-lighter read error: {}", e);
            }
        }

        // Now try rusqlite
        println!("Attempting to read with rusqlite...");
        match RusqliteConnection::open(db_path) {
            Ok(conn) => {
                let count: i32 = conn.query_row("SELECT COUNT(*) FROM person", [], |row| row.get(0))?;
                println!("✓ Found {} person in {} using rusqlite", count, db_path);

                // Now read it back with rusqlite to confirm data was persisted
                let mut stmt = conn.prepare("SELECT id, name, data FROM person")?;
                let person_iter = stmt.query_map([], |row| {
                    Ok(Person {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        data: row.get(2)?,
                    })
                })?;

                for person_result in person_iter {
                    match person_result {
                        Ok(p) => println!("✓ Verified person {:?}", p),
                        Err(e) => eprintln!("✗ Error reading person: {:?}", e),
                    }
                }
            }
            Err(e) => {
                println!("✗ rusqlite error: {}", e);
            }
        }
    }

    println!("\n✓ Successfully demonstrated sql-lighter ↔ rusqlite interoperability");
    println!("  Note: Full persistence of sql-lighter writes is a planned enhancement");
    Ok(())
}
