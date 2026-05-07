//! Write person data using rusqlite
//! Creates a database file that can be read by sql-lighter

#[derive(Debug)]
struct Person {
    id: i32,
    name: String,
    data: Option<String>,
}

fn main() -> rusqlite::Result<()> {
    println!("Starting person_write_rusqlite example...");
    
    // Create or open database
    println!("Opening person.db with rusqlite...");
    let conn = rusqlite::Connection::open("person.db")?;
    println!("✓ Database opened successfully");

    // Create table
    println!("Creating table...");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS person (
            id   INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            data TEXT
        )",
        [],
    )?;
    println!("✓ Table created");

    // Clear existing data
    println!("Clearing existing data...");
    conn.execute("DELETE FROM person", [])?;
    println!("✓ Data cleared");

    // Insert test data
    let people = vec![
        Person {
            id: 1,
            name: "Alice".to_string(),
            data: Some("Engineer".to_string()),
        },
        Person {
            id: 2,
            name: "Bob".to_string(),
            data: None,
        },
        Person {
            id: 3,
            name: "Charlie".to_string(),
            data: Some("Manager".to_string()),
        },
    ];

    println!("Inserting {} people...", people.len());
    for person in people {
        conn.execute(
            "INSERT INTO person (id, name, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![person.id, person.name, person.data],
        )?;
    }
    println!("✓ People inserted");

    // Count rows
    let count: i32 = conn.query_row("SELECT COUNT(*) FROM person", [], |row| {
        row.get(0)
    })?;
    println!("✓ Written {} rows to person.db using rusqlite", count);

    Ok(())
}
