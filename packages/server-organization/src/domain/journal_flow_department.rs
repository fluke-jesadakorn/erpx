//! Organization-owned display facts for a retained journal Flow roster.
//!
//! The adapter owns the SQL and transaction fence; this module owns the
//! retained fallback rule so it can be tested without PostgreSQL.
pub fn team_name_from_department(display_name: Option<&str>) -> String {
    match display_name {
        Some(name) if !name.trim().is_empty() => name.to_owned(),
        _ => "Company".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::team_name_from_department;

    #[test]
    fn missing_or_blank_department_uses_company() {
        assert_eq!(team_name_from_department(None), "Company");
        assert_eq!(team_name_from_department(Some("")), "Company");
        assert_eq!(team_name_from_department(Some("   ")), "Company");
    }

    #[test]
    fn active_department_name_is_preserved() {
        assert_eq!(
            team_name_from_department(Some("Engineering")),
            "Engineering"
        );
    }
}
