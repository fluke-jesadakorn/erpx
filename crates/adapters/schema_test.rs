//! Test support for assertions over one owned SQL schema inside a clean baseline.

/// Select complete SQL statements owned by one of `schemas`.
///
/// Clean-slate migrations intentionally contain the whole database in one file.
/// Adapter contract tests must therefore inspect only their own schema instead
/// of accidentally accepting or rejecting a column owned by another context.
pub fn select_schema_statements(ddl: &str, schemas: &[&str]) -> String {
    let needles = schemas
        .iter()
        .map(|schema| format!("{}.", schema.to_ascii_lowercase()))
        .collect::<Vec<_>>();

    select_statements(ddl, &needles.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Select complete SQL statements containing an owned relation prefix or marker.
pub fn select_statements(ddl: &str, needles: &[&str]) -> String {
    let needles = needles
        .iter()
        .map(|needle| needle.to_ascii_lowercase())
        .collect::<Vec<_>>();

    ddl.split_inclusive(';')
        .filter(|statement| {
            let normalized = statement.to_ascii_lowercase();
            needles.iter().any(|needle| normalized.contains(needle))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return one complete `CREATE TABLE` definition from a baseline.
#[allow(dead_code)]
pub fn table_definition<'a>(ddl: &'a str, qualified_table: &str) -> Option<&'a str> {
    ddl.split(&format!("CREATE TABLE {qualified_table}"))
        .nth(1)
        .and_then(|tail| tail.split_once("\n);"))
        .map(|(definition, _)| definition)
}

#[cfg(test)]
mod tests {
    use super::select_schema_statements;

    #[test]
    fn selects_whole_owned_statements_without_cross_context_columns() {
        let ddl = "CREATE TABLE alpha.one (id UUID);\nCREATE TABLE beta.two (state JSONB);";
        let alpha = select_schema_statements(ddl, &["alpha"]);
        assert!(alpha.contains("alpha.one"));
        assert!(!alpha.contains("state JSONB"));
    }
}
