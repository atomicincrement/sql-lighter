use sql_lighter::cli::Cli;

mod cli;

fn main() {
    let cli = Cli::default();
    println!("SQL Lighter v{}", env!("CARGO_PKG_VERSION"));
    println!("Type 'help' for usage information");
    cli.run();
}
