use clap::ArgMatches;
use diesel::expression::sql;
#[cfg(feature = "postgres")]
use diesel::pg::PgConnection;
#[cfg(feature = "sqlite")]
use diesel::sqlite::SqliteConnection;
use diesel::types::Bool;
use diesel::{migrations, Connection, select, LoadDsl};

use database_error::DatabaseResult;

use std::error::Error;
use std::{env, fs};

use std::path::Path;

// FIXME: Remove the duplicates of this macro once expression level attributes
// are stable (I believe this is in 1.7)
#[cfg(all(feature = "sqlite", feature = "postgres"))]
macro_rules! call_with_conn {
    ( $database_url:ident,
      $func:path
    ) => {{
        match ::database::backend(&$database_url) {
            "postgres" => {
                let conn = PgConnection::establish(&$database_url).unwrap();
                $func(&conn)
            },
            "sqlite" => {
                let conn = SqliteConnection::establish(&$database_url).unwrap();
                $func(&conn)
            },
            _ => unreachable!("The backend function should ensure we never get here."),
        }
    }};
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
macro_rules! call_with_conn {
    ( $database_url:ident,
      $func:path
    ) => {{
        let conn = SqliteConnection::establish(&$database_url).unwrap();
        $func(&conn)
    }};
}

#[cfg(all(not(feature = "sqlite"), feature = "postgres"))]
macro_rules! call_with_conn {
    ( $database_url:ident,
      $func:path
    ) => {{
        let conn = PgConnection::establish(&$database_url).unwrap();
        $func(&conn)
    }};
}

pub fn reset_database(args: &ArgMatches) -> DatabaseResult<()> {
    try!(drop_database(&database_url(args)));
    setup_database(args)
}

pub fn setup_database(args: &ArgMatches) -> DatabaseResult<()> {
    let database_url = database_url(args);

    try!(create_database_if_needed(&database_url));
    create_schema_table_and_run_migrations_if_needed(&database_url)
}

pub fn drop_database_command(args: &ArgMatches) -> DatabaseResult<()> {
    drop_database(&database_url(args))
}

// FIXME: Remove the duplicates of this function once expression level attributes
// are stable (I believe this is in 1.7)
/// Creates the database specified in the connection url. It returns an error
/// it it was unable to create the database.
#[cfg(all(feature = "sqlite", feature = "postgres"))]
fn create_database_if_needed(database_url: &String) -> DatabaseResult<()> {
    match backend(database_url) {
        "postgres" => {
            if PgConnection::establish(database_url).is_err() {
                let (database, postgres_url) = split_pg_connection_string(database_url);
                println!("Creating database: {}", database);
                let conn = try!(PgConnection::establish(&postgres_url));
                try!(conn.execute(&format!("CREATE DATABASE {}", database)));
            }
        },
        "sqlite" => {
            if !Path::new(database_url).exists() {
                println!("Creating database: {}", database_url);
                try!(SqliteConnection::establish(database_url));
            }
        },
        _ => unreachable!("The backend function should ensure we never get here."),
    }

    Ok(())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn create_database_if_needed(database_url: &String) -> DatabaseResult<()> {
    if !Path::new(database_url).exists() {
        println!("Creating database: {}", database_url);
        try!(SqliteConnection::establish(database_url));
    }
    Ok(())
}

#[cfg(all(not(feature = "sqlite"), feature = "postgres"))]
fn create_database_if_needed(database_url: &String) -> DatabaseResult<()> {
    if PgConnection::establish(database_url).is_err() {
        let (database, postgres_url) = split_pg_connection_string(database_url);
        println!("Creating database: {}", database);
        let conn = try!(PgConnection::establish(&postgres_url));
        try!(conn.execute(&format!("CREATE DATABASE {}", database)));
    }
    Ok(())
}

/// Creates the __diesel_schema_migrations table if it doesn't exist. If the
/// table didn't exist, it also runs any pending migrations. Returns a
/// `DatabaseError::ConnectionError` if it can't create the table, and exits
/// with a migration error if it can't run migrations.
fn create_schema_table_and_run_migrations_if_needed(database_url: &String)
    -> DatabaseResult<()>
{
    if !schema_table_exists(database_url).map_err(handle_error).unwrap() {
        try!(call_with_conn!(database_url, migrations::setup_database));
        call_with_conn!(database_url, migrations::run_pending_migrations).unwrap_or_else(handle_error);
    };
    Ok(())
}

// FIXME: Remove the duplicates of this function once expression level attributes
// are stable (I believe this is in 1.7)
/// Drops the database specified in the connection url. It returns an error
/// if it was unable to drop the database.
#[cfg(all(feature = "sqlite", feature = "postgres"))]
fn drop_database(database_url: &String) -> DatabaseResult<()> {
    match backend(database_url) {
        "postgres" => {
            let (database, postgres_url) = split_pg_connection_string(database_url);
            println!("Dropping database: {}", database);
            let conn = try!(PgConnection::establish(&postgres_url));
            try!(conn.silence_notices(|| {
                conn.execute(&format!("DROP DATABASE IF EXISTS {}", database))
            }));
        },
        "sqlite" => {
            println!("Dropping database: {}", database_url);
            try!(fs::remove_file(&database_url));
        },
        _ => unreachable!("The backend function should ensure we never get here."),
    }
    Ok(())
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
fn drop_database(database_url: &String) -> DatabaseResult<()> {
    println!("Dropping database: {}", database_url);
    try!(fs::remove_file(&database_url));
    Ok(())
}

#[cfg(all(not(feature = "sqlite"), feature = "postgres"))]
fn drop_database(database_url: &String) -> DatabaseResult<()> {
    let (database, postgres_url) = split_pg_connection_string(database_url);
    println!("Dropping database: {}", database);
    let conn = try!(PgConnection::establish(&postgres_url));
    try!(conn.silence_notices(|| {
        conn.execute(&format!("DROP DATABASE IF EXISTS {}", database))
    }));
    Ok(())
}

// FIXME: Remove the duplicates of this function once expression level attributes
// are stable (I believe this is in 1.7)
/// Returns true if the '__diesel_schema_migrations' table exists in the
/// database we connect to, returns false if it does not.
#[cfg(all(feature = "sqlite", feature = "postgres"))]
pub fn schema_table_exists(database_url: &String) -> DatabaseResult<bool> {
    let result = match backend(database_url) {
        "postgres" => {
            let conn = PgConnection::establish(database_url).unwrap();
            try!(select(sql::<Bool>("EXISTS \
                    (SELECT 1 \
                     FROM information_schema.tables \
                     WHERE table_name = '__diesel_schema_migrations')"))
                .get_result(&conn))
        },
        "sqlite" => {
            let conn = SqliteConnection::establish(database_url).unwrap();
            try!(select(sql::<Bool>("EXISTS \
                    (SELECT 1 \
                     FROM sqlite_master \
                     WHERE type = 'table' \
                     AND name = '__diesel_schema_migrations')"))
                .get_result(&conn))
        },
        _ => unreachable!("The backend function should ensure we never get here."),
    };
    Ok(result)
}

#[cfg(all(feature = "sqlite", not(feature = "postgres")))]
pub fn schema_table_exists(database_url: &String) -> DatabaseResult<bool> {
    let conn = SqliteConnection::establish(database_url).unwrap();
    select(sql::<Bool>("EXISTS \
            (SELECT 1 \
             FROM sqlite_master \
             WHERE type = 'table' \
             AND name = '__diesel_schema_migrations')"))
        .get_result(&conn)
        .map_err(|e| e.into())
}

#[cfg(all(not(feature = "sqlite"), feature = "postgres"))]
pub fn schema_table_exists(database_url: &String) -> DatabaseResult<bool> {
    let conn = PgConnection::establish(database_url).unwrap();
    select(sql::<Bool>("EXISTS \
            (SELECT 1 \
             FROM information_schema.tables \
             WHERE table_name = '__diesel_schema_migrations')"))
        .get_result(&conn)
        .map_err(|e| e.into())
}

pub fn database_url(matches: &ArgMatches) -> String {
    matches.value_of("DATABASE_URL")
        .map(|s| s.into())
        .or(env::var("DATABASE_URL").ok())
        .expect("The --database-url argument must be passed, \
                or the DATABASE_URL environment variable must be set.")
}

/// Returns a &str representing the type of backend being used, determined
/// by the format of the database url.
pub fn backend(database_url: &String) -> &str {
    if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        "postgres"
    } else {
        "sqlite"
    }
}

fn split_pg_connection_string(database_url: &String) -> (String, String) {
    let mut split: Vec<&str> = database_url.split("/").collect();
    let database = split.pop().unwrap();
    let postgres_url = split.join("/");
    (database.to_owned(), postgres_url)
}

fn handle_error<E: Error>(error: E) {
    panic!("{}", error);
}

#[cfg(test)]
mod tests {
    use super::split_pg_connection_string;

    #[test]
    fn split_pg_connection_string_returns_postgres_url_and_database() {
        let database = "database".to_owned();
        let postgres_url = "postgresql://localhost:5432".to_owned();
        let database_url = format!("{}/{}", postgres_url, database);
        assert_eq!((database, postgres_url), split_pg_connection_string(&database_url));
    }
}
