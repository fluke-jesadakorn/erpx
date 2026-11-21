//! Owner Postgres adapter for native Procurement write statements. The only place
//! in the migrated path that holds SQL text; it executes admitted statements on
//! the caller-owned serializable connection and never opens a transaction.
use crate::application::native_command::{NativeCommandFuture, NativeCommandRepository};
use crate::domain::native_command;
use serde_json::Value;
use sqlx::postgres::PgArguments;
use sqlx::query::Query;
use sqlx::{AssertSqlSafe, Connection, PgConnection};

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
            Some(integer) => query.bind(integer),
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

impl NativeCommandRepository for PgNativeCommandRepository<'_> {
    fn connection(&mut self) -> Option<&mut PgConnection> {
        Some(&mut *self.connection)
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'static str,
        values: Vec<Value>,
        _key: &'a str,
    ) -> NativeCommandFuture<'a, u64> {
        Box::pin(async move {
            if !self.connection.is_in_transaction() {
                return Err(native_command::unavailable());
            }
            let query = bind_batch(sqlx::query(AssertSqlSafe(sql)), values);
            let result = query
                .execute(&mut *self.connection)
                .await
                .map_err(|_| native_command::unavailable())?;
            Ok(result.rows_affected())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalar_and_array_values_are_accepted_by_the_binder() {
        let mut query = sqlx::query(AssertSqlSafe(
            "UPDATE t SET a=$1,b=$2,c=$3,d=$4,e=$5 WHERE id=$6",
        ));
        for value in [
            json!("text"),
            json!(42),
            json!(1.5),
            json!(true),
            json!(["a", "b"]),
            json!(null),
            json!({"k": 1}),
        ] {
            query = bind(query, value);
        }
        let _ = query;
    }
}
