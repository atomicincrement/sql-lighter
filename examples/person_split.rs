//! Split person example: write with rusqlite, read with sql-lighter
//! Demonstrates interoperability by writing a person to a database file using rusqlite,
//! then reading it back using sql-lighter's Connection API.

use rusqlite::Connection as RusqliteConnection;
use sql_lighter::connection::Connection;

#[derive(Debug)]
struct Person {
    id: i32,
    name: String,
    data: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "target/person_split.db";

    // ===== PART 1: Write using rusqlite =====
    println!("=== Writing with rusqlite ===");
    {
        let conn = RusqliteConnection::open(db_path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS person (
                id   INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                data TEXT
            )",
            [],
        )?;

        conn.execute("DELETE FROM person", [])?;

        // Write a single person
        let person = Person {
            id: 1,
            name: "Steven".to_string(),
            data: None,
        };

        conn.execute(
            "INSERT INTO person (id, name, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![person.id, person.name, person.data],
        )?;

        let count: i32 = conn.query_row("SELECT COUNT(*) FROM person", [], |row| row.get(0))?;
        println!("✓ Written {} person to {} using rusqlite", count, db_path);
    }

    // ===== PART 2: Read using sql-lighter =====
    println!("\n=== Reading with sql-lighter ===");
    {
        // Open the file-based database created by rusqlite
        // The data is automatically loaded from the B-tree storage
        let mut conn = Connection::open(db_path)?;

        // Read and display the person using prepare/query_map
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
                Ok(p) => println!("✓ Found person {:?}", p),
                Err(e) => eprintln!("✗ Error reading person: {:?}", e),
            }
        }
    }

    println!("\n✓ Successfully demonstrated rusqlite ↔ sql-lighter interoperability");
    Ok(())
}
