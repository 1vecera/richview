//! Test support, not a product surface: creates a fresh DuckDB file from a seed script with the
//! bundled duckdb crate, so the gate can render `examples/sql-report` without a duckdb binary.
//! Usage: `seed_duckdb <seed.sql> <database>`. An existing database is never overwritten.
//! With `:memory:` as the database the script runs against an in-memory one and keeps only the
//! files it writes itself, as the seed of `examples/parquet-report` writes its Parquet files.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [seed, database] = args.as_slice() else {
        return Err("usage: seed_duckdb <seed.sql> <database>".into());
    };
    if std::path::Path::new(database).exists() {
        return Err(format!("{database} already exists; refusing to overwrite it").into());
    }
    let script = std::fs::read_to_string(seed)?;
    duckdb::Connection::open(database)?.execute_batch(&script)?;
    Ok(())
}
