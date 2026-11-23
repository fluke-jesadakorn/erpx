//! Owner Postgres adapter for native marketing commands. The only place in the
//! migrated path that holds SQL text; it executes admitted statements on the
//! caller-owned serializable connection and never opens a transaction.
use crate::application::native_command::{NativeCommandFuture, NativeCommandRepository};
use crate::domain::native_command;
use serde_json::{Map, Value};
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::Query;
use sqlx::{AssertSqlSafe, Column, Connection, PgConnection, Row};

pub struct PgNativeCommandRepository<'a> {
    connection: &'a mut PgConnection,
}
impl<'a> PgNativeCommandRepository<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

type BoundQuery<'q> = Query<'q, sqlx::Postgres, PgArguments>;
fn bind(query: BoundQuery<'_>, value: Value) -> BoundQuery<'_> {
    match value {
        Value::Null => query.bind(Option::<&str>::None),
        Value::Bool(flag) => query.bind(flag),
        Value::Number(number) => match number.as_i64() {
            Some(i) => query.bind(i),
            None => query.bind(number.as_f64().unwrap_or(0.0)),
        },
        Value::String(text) => query.bind(text),
        Value::Array(items) => {
            if items.iter().all(|i| matches!(i, Value::String(_))) {
                query.bind(
                    items
                        .into_iter()
                        .map(|i| i.as_str().unwrap_or_default().to_owned())
                        .collect::<Vec<_>>(),
                )
            } else if items.iter().all(|i| matches!(i, Value::Number(_))) {
                query.bind(
                    items
                        .into_iter()
                        .filter_map(|i| i.as_i64())
                        .collect::<Vec<_>>(),
                )
            } else {
                query.bind(Value::Array(items).to_string())
            }
        }
        Value::Object(_) => query.bind(value.to_string()),
    }
}
fn bind_batch(mut query: BoundQuery<'_>, values: Vec<Value>) -> BoundQuery<'_> {
    for value in values {
        query = bind(query, value);
    }
    query
}
/// Decode one returned row into a JSON object keyed by column name. The
/// admitted statements only RETURN scalar columns (the public-limit count),
/// but decoding by column name keeps the transport generic for future rows.
fn row_json(row: &PgRow) -> Value {
    let mut object = Map::new();
    for column in row.columns() {
        let name = column.name();
        let value = row
            .try_get::<Option<i64>, _>(name)
            .ok()
            .flatten()
            .map(Value::from)
            .or_else(|| row.try_get::<Option<f64>, _>(name).ok().flatten().map(Value::from))
            .or_else(|| row.try_get::<Option<bool>, _>(name).ok().flatten().map(Value::from))
            .or_else(|| row.try_get::<Option<String>, _>(name).ok().flatten().map(Value::from))
            .or_else(|| {
                row.try_get::<Option<sqlx::types::Json<Value>>, _>(name)
                    .ok()
                    .flatten()
                    .map(|json| json.0)
            })
            .unwrap_or(Value::Null);
        object.insert(name.to_owned(), value);
    }
    Value::Object(object)
}
impl NativeCommandRepository for PgNativeCommandRepository<'_> {
    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }
    fn execute<'a>(
        &'a mut self,
        sql: &'static str,
        values: Vec<Value>,
        _key: &'a str,
    ) -> NativeCommandFuture<'a, Vec<Value>> {
        Box::pin(async move {
            if !self.connection.is_in_transaction() {
                return Err(native_command::unavailable());
            }
            let query = bind_batch(sqlx::query(AssertSqlSafe(sql)), values);
            // `fetch_all` executes the statement and buffers its RETURNING rows.
            // A statement without RETURNING executes normally and yields no rows.
            let returned = query
                .fetch_all(&mut *self.connection)
                .await
                .map_err(|_| native_command::unavailable())?;
            Ok(returned.iter().map(row_json).collect())
        })
    }
}
