#[test]
fn debug_query_with_params() {
    use sql_lighter::connection::Connection;
    
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute("CREATE TABLE items (id INTEGER, name TEXT)", ()).unwrap();
    
    // Insert test data
    conn.execute("INSERT INTO items VALUES (?1, ?2)", (1i32, "item1")).unwrap();
    
    // Query all rows
    let all_rows = conn.query("SELECT * FROM items", ()).unwrap();
    println!("All rows: {:?}", all_rows);
    
    // Query with WHERE clause
    let filtered_rows = conn.query("SELECT * FROM items WHERE id = ?1", (1i32,)).unwrap();
    println!("Filtered rows: {:?}", filtered_rows);
}
