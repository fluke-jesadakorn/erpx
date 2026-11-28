//! Organization-owned display facts for a retained journal Flow roster.
//!
//! The adapter owns the SQL and transaction fence; this module owns the
//! retained fallback rule so it can be tested without PostgreSQL.
pub fn team_name_from_department(display_name: Option<&str>) -> String {
    display_name.unwrap_or("Company").to_owned()
}

#[cfg(test)]
mod tests {
    use super::team_name_from_department;

    #[test]
    fn only_missing_department_uses_company() {
        assert_eq!(team_name_from_department(None), "Company");
        assert_eq!(team_name_from_department(Some("")), "");
        assert_eq!(team_name_from_department(Some("   ")), "   ");
    }

    #[test]
    fn active_department_name_is_preserved() {
        assert_eq!(
            team_name_from_department(Some("Engineering")),
            "Engineering"
        );
    }
}
