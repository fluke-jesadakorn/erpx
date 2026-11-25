//! Owner PostgreSQL execution for the lane 8 read family. The statements are
//! the byte-identical owner SQL from the retired TypeScript call sites with the
//! only execution-time adaptation being D1 `?` binds rewritten to sqlx/Postgres
//! `$n` binds. Bind order, predicates, projection and row order are unchanged;
//! the Company scope is always the first bind.

use erpx_server_shared::public::BusinessError;
use sha2::{Digest, Sha256};
use sqlx::Row;

use super::application::{AppEntrypointRead, LaneReadFuture};
use super::domain::{
    ApproverName, CountryPackRow, ExportBytesRow, IssuableRun, OwnSlipRow, PayGroup, PayGroupFacts,
    PaymentRow, PayrollItemTotals, PersonOption, PayslipItemRow, ReadKind, RosterShift, SlipRow,
    StatutoryRun, unavailable,
};

use erpx_server_runtime::public::postgres::DatabasePool;

const PAY_GROUPS_SQL: &str =
    "SELECT id,name,currency,frequency FROM hr_pay_groups WHERE company_id=$1 AND status='active' ORDER BY name,id";
const PEOPLE_SQL: &str =
    "SELECT id,legal_name,email,version FROM hr_people WHERE company_id=$1 AND status='active' AND merged_into_id IS NULL ORDER BY legal_name,id";
const APPROVER_NAME_SQL: &str =
    "SELECT legal_name FROM hr_people WHERE company_id=$1 AND iam_same_person(user_id,$2,company_id) AND merged_into_id IS NULL LIMIT 1";
const ROSTER_SHIFT_SQL: &str = "SELECT s.id,s.local_date,s.start_time,s.end_time,w.timezone,to_char(to_timestamp(s.ends_at/1000.0) AT TIME ZONE w.timezone,'YYYY-MM-DD\"T\"HH24:MI') end_local FROM people_roster_shifts s JOIN people_roster_weeks w ON w.id=s.week_id JOIN people_roster_assignments a ON a.shift_id=s.id WHERE s.id=$1 AND w.company_id=$2 AND w.state='published' AND s.state='active' AND a.engagement_id=$3 AND a.state='assigned'";

// Payslip family. These are the byte-identical owner statements (D1 `?`
// rewritten to sqlx `$n`). `own_slips`/`slip` keep the caller's optional
// predicates; their canonical broad+filtered rendering is the oracle text.
const ISSUABLE_RUN_SQL: &str =
    "SELECT * FROM hr_payroll_runs WHERE id=$1 AND company_id=$2 AND status IN ('approved','posted')";
const ISSUABLE_ITEMS_SQL: &str = "SELECT i.*,e.person_id,p.user_id recipient_user_id FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id AND e.company_id=$1 JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE i.run_id=$2 AND i.worker_type='employee' AND i.status IN ('calculated','approved','posted') ORDER BY i.id";
const OWN_SLIPS_SQL: &str = "SELECT s.id,s.run_id,s.run_revision,s.pay_date,s.currency,s.gross_minor,s.deductions_minor,s.net_minor,s.source_digest,s.export_sha256,s.issued_at,s.person_id,s.snapshot_json FROM hr_issued_payslips s JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.company_id=$1 AND s.run_id=$2 AND s.recipient_user_id=$3 AND p.user_id=$4 AND p.merged_into_id IS NULL ORDER BY s.pay_date DESC,s.issued_at DESC,s.id";
const SLIP_SQL: &str = "SELECT s.* FROM hr_issued_payslips s JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.id=$1 AND s.company_id=$2 AND s.recipient_user_id=$3 AND p.user_id=$4 AND p.merged_into_id IS NULL";
const PAYMENT_SQL: &str = "SELECT i.status,i.settled_at,b.status batch_status FROM hr_payment_batch_items i JOIN hr_payment_batches b ON b.id=i.batch_id WHERE b.company_id=$1 AND b.source_type='payroll' AND b.source_id=$2 AND i.person_id=$3 AND b.status!='cancelled' ORDER BY b.created_at DESC LIMIT 1";
const EXPORT_BYTES_SQL: &str =
    "SELECT export_html,export_sha256 FROM hr_issued_payslips WHERE id=$1 AND company_id=$2";
const COUNTRY_PACK_SQL: &str =
    "SELECT id,country_code,version,status,rules_json,adapter_id,source_digest,record_version FROM hr_country_packs WHERE id=$1";
const PAY_GROUP_FACTS_SQL: &str =
    "SELECT country_code,currency,frequency FROM hr_pay_groups WHERE id=$1 AND company_id=$2 AND status='active'";
const STATUTORY_RUN_SQL: &str =
    "SELECT mode,country_pack_id,currency FROM hr_payroll_runs WHERE id=$1";
const STATUTORY_ITEMS_SQL: &str =
    "SELECT gross_minor,deductions_minor,net_minor,result_json FROM hr_payroll_items WHERE run_id=$1";
const RETAINED_PACK_SQL: &str =
    "SELECT * FROM hr_country_packs WHERE id=$1 AND status IN ('active','superseded')";

/// The exact statement owned by each read kind; exposed for the SQL oracle.
pub fn rendered_sql(kind: ReadKind) -> &'static str {
    match kind {
        ReadKind::PayGroups => PAY_GROUPS_SQL,
        ReadKind::People => PEOPLE_SQL,
        ReadKind::ApproverName => APPROVER_NAME_SQL,
        ReadKind::RosterShift => ROSTER_SHIFT_SQL,
        ReadKind::IssuableRun => ISSUABLE_RUN_SQL,
        ReadKind::IssuableItems => ISSUABLE_ITEMS_SQL,
        ReadKind::OwnSlips => OWN_SLIPS_SQL,
        ReadKind::Slip => SLIP_SQL,
        ReadKind::Payment => PAYMENT_SQL,
        ReadKind::ExportBytes => EXPORT_BYTES_SQL,
        ReadKind::CountryPack => COUNTRY_PACK_SQL,
        ReadKind::PayGroupFacts => PAY_GROUP_FACTS_SQL,
        ReadKind::StatutoryRun => STATUTORY_RUN_SQL,
        ReadKind::StatutoryItems => STATUTORY_ITEMS_SQL,
        ReadKind::RetainedPack => RETAINED_PACK_SQL,
    }
}

fn sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Stable SQL/result oracle over a read kind, its exact statement and the
/// canonical projected result. Used by the Rust and TypeScript parity tests.
pub fn oracle(kind: ReadKind, result: &str) -> String {
    sha256(&format!("{}|{}|{}", kind.as_str(), rendered_sql(kind), result))
}

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<String, BusinessError> {
    if let Ok(Some(value)) = row.try_get::<Option<String>, _>(column) {
        return Ok(value);
    }
    if let Ok(Some(value)) = row.try_get::<Option<i64>, _>(column) {
        return Ok(value.to_string());
    }
    row.try_get::<Option<i32>, _>(column)
        .ok()
        .flatten()
        .map(|value| value.to_string())
        .ok_or_else(unavailable)
}

fn option_text(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>, BusinessError> {
    if let Ok(value) = row.try_get::<Option<String>, _>(column) {
        return Ok(value);
    }
    if let Ok(value) = row.try_get::<Option<i64>, _>(column) {
        return Ok(value.map(|item| item.to_string()));
    }
    row.try_get::<Option<i32>, _>(column)
        .map(|value| value.map(|item| item.to_string()))
        .map_err(|_| unavailable())
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<i64, _>(column)
        .or_else(|_| row.try_get::<i32, _>(column).map(i64::from))
        .map_err(|_| unavailable())
}

fn pack_row(row: &sqlx::postgres::PgRow) -> Result<CountryPackRow, BusinessError> {
    Ok(CountryPackRow {
        id: text(row, "id")?,
        country_code: text(row, "country_code")?,
        version: integer(row, "version")?,
        status: text(row, "status")?,
        rules_json: text(row, "rules_json")?,
        adapter_id: text(row, "adapter_id")?,
        source_digest: text(row, "source_digest")?,
        record_version: integer(row, "record_version")?,
    })
}

/// Pg-backed owner reader. Only People Payroll-owned tables are touched; no
/// permission, stock or command authority is inferred from these reads.
pub struct PgAppEntrypointRead {
    pool: DatabasePool,
}

impl PgAppEntrypointRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AppEntrypointRead for PgAppEntrypointRead {
    fn pay_groups<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<PayGroup>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(PAY_GROUPS_SQL)
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(PayGroup {
                        id: text(row, "id")?,
                        name: text(row, "name")?,
                        currency: text(row, "currency")?,
                        frequency: text(row, "frequency")?,
                    })
                })
                .collect()
        })
    }

    fn people<'a>(&'a self, company: &'a str) -> LaneReadFuture<'a, Vec<PersonOption>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(PEOPLE_SQL)
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(PersonOption {
                        id: text(row, "id")?,
                        legal_name: text(row, "legal_name")?,
                        email: option_text(row, "email")?,
                        version: integer(row, "version")?,
                    })
                })
                .collect()
        })
    }

    fn approver_name<'a>(
        &'a self,
        company: &'a str,
        user: &'a str,
    ) -> LaneReadFuture<'a, Option<ApproverName>> {
        let pool = self.pool.clone();
        let (company, user) = (company.to_owned(), user.to_owned());
        Box::pin(async move {
            let row = sqlx::query(APPROVER_NAME_SQL)
                .bind(company.as_str())
                .bind(user.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| Ok(ApproverName { legal_name: text(&row, "legal_name")? }))
                .transpose()
        })
    }

    fn roster_shift<'a>(
        &'a self,
        company: &'a str,
        shift: &'a str,
        engagement: &'a str,
    ) -> LaneReadFuture<'a, Option<RosterShift>> {
        let pool = self.pool.clone();
        let (company, shift, engagement) =
            (company.to_owned(), shift.to_owned(), engagement.to_owned());
        Box::pin(async move {
            let row = sqlx::query(ROSTER_SHIFT_SQL)
                .bind(shift.as_str())
                .bind(company.as_str())
                .bind(engagement.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(RosterShift {
                    id: text(&row, "id")?,
                    local_date: text(&row, "local_date")?,
                    start_time: text(&row, "start_time")?,
                    end_time: option_text(&row, "end_time")?,
                    end_local: text(&row, "end_local")?,
                    timezone: text(&row, "timezone")?,
                })
            })
            .transpose()
        })
    }

    fn issuable_run<'a>(
        &'a self,
        company: &'a str,
        run: &'a str,
    ) -> LaneReadFuture<'a, Option<IssuableRun>> {
        let pool = self.pool.clone();
        let (company, run) = (company.to_owned(), run.to_owned());
        Box::pin(async move {
            let row = sqlx::query(ISSUABLE_RUN_SQL)
                .bind(run.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(IssuableRun {
                    id: text(&row, "id")?,
                    input_digest: text(&row, "input_digest")?,
                    revision: integer(&row, "revision")?,
                    version: integer(&row, "version")?,
                    mode: text(&row, "mode")?,
                    period_start: text(&row, "period_start")?,
                    period_end: text(&row, "period_end")?,
                    pay_date: text(&row, "pay_date")?,
                    currency: text(&row, "currency")?,
                })
            })
            .transpose()
        })
    }

    fn issuable_items<'a>(
        &'a self,
        company: &'a str,
        run: &'a str,
    ) -> LaneReadFuture<'a, Vec<PayslipItemRow>> {
        let pool = self.pool.clone();
        let (company, run) = (company.to_owned(), run.to_owned());
        Box::pin(async move {
            let rows = sqlx::query(ISSUABLE_ITEMS_SQL)
                .bind(company.as_str())
                .bind(run.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(PayslipItemRow {
                        id: text(row, "id")?,
                        engagement_id: text(row, "engagement_id")?,
                        person_id: text(row, "person_id")?,
                        recipient_user_id: option_text(row, "recipient_user_id")?,
                        input_snapshot_json: text(row, "input_snapshot_json")?,
                        result_json: text(row, "result_json")?,
                        gross_minor: integer(row, "gross_minor")?,
                        deductions_minor: integer(row, "deductions_minor")?,
                        employer_cost_minor: integer(row, "employer_cost_minor")?,
                        net_minor: integer(row, "net_minor")?,
                        currency: text(row, "currency")?,
                    })
                })
                .collect()
        })
    }

    fn own_slips<'a>(
        &'a self,
        company: &'a str,
        run: Option<&'a str>,
        user: Option<&'a str>,
    ) -> LaneReadFuture<'a, Vec<OwnSlipRow>> {
        let pool = self.pool.clone();
        let (company, run, user) = (
            company.to_owned(),
            run.map(str::to_owned),
            user.map(str::to_owned),
        );
        Box::pin(async move {
            let mut sql = String::from(
                "SELECT s.id,s.run_id,s.run_revision,s.pay_date,s.currency,s.gross_minor,\
                 s.deductions_minor,s.net_minor,s.source_digest,s.export_sha256,s.issued_at,\
                 s.person_id,s.snapshot_json FROM hr_issued_payslips s \
                 JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id \
                 WHERE s.company_id=$1",
            );
            let mut next = 2;
            if run.is_some() {
                sql.push_str(&format!(" AND s.run_id=${next}"));
                next += 1;
            }
            if user.is_some() {
                sql.push_str(&format!(" AND s.recipient_user_id=${next}"));
                next += 1;
                sql.push_str(&format!(
                    " AND p.user_id=${next} AND p.merged_into_id IS NULL"
                ));
            }
            sql.push_str(" ORDER BY s.pay_date DESC,s.issued_at DESC,s.id");
            let mut statement = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(company.as_str());
            if let Some(run) = &run {
                statement = statement.bind(run.as_str());
            }
            if let Some(user) = &user {
                statement = statement.bind(user.as_str()).bind(user.as_str());
            }
            let rows = statement.fetch_all(pool.raw()).await.map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(OwnSlipRow {
                        id: text(row, "id")?,
                        run_id: text(row, "run_id")?,
                        run_revision: integer(row, "run_revision")?,
                        pay_date: text(row, "pay_date")?,
                        currency: text(row, "currency")?,
                        gross_minor: integer(row, "gross_minor")?,
                        deductions_minor: integer(row, "deductions_minor")?,
                        net_minor: integer(row, "net_minor")?,
                        source_digest: text(row, "source_digest")?,
                        export_sha256: text(row, "export_sha256")?,
                        issued_at: integer(row, "issued_at")?,
                        snapshot_json: text(row, "snapshot_json")?,
                    })
                })
                .collect()
        })
    }

    fn slip<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
        user: Option<&'a str>,
    ) -> LaneReadFuture<'a, Option<SlipRow>> {
        let pool = self.pool.clone();
        let (company, id, user) = (
            company.to_owned(),
            id.to_owned(),
            user.map(str::to_owned),
        );
        Box::pin(async move {
            let mut sql = String::from(
                "SELECT s.* FROM hr_issued_payslips s \
                 JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id \
                 WHERE s.id=$1 AND s.company_id=$2",
            );
            if user.is_some() {
                sql.push_str(" AND s.recipient_user_id=$3 AND p.user_id=$4 AND p.merged_into_id IS NULL");
            }
            let mut statement = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(id.as_str())
                .bind(company.as_str());
            if let Some(user) = &user {
                statement = statement.bind(user.as_str()).bind(user.as_str());
            }
            let row = statement.fetch_optional(pool.raw()).await.map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(SlipRow {
                    id: text(&row, "id")?,
                    run_id: text(&row, "run_id")?,
                    person_id: text(&row, "person_id")?,
                    recipient_user_id: option_text(&row, "recipient_user_id")?,
                    snapshot_json: text(&row, "snapshot_json")?,
                    export_html: text(&row, "export_html")?,
                    export_sha256: text(&row, "export_sha256")?,
                    issued_at: integer(&row, "issued_at")?,
                })
            })
            .transpose()
        })
    }

    fn payment<'a>(
        &'a self,
        company: &'a str,
        run: &'a str,
        person: &'a str,
    ) -> LaneReadFuture<'a, Option<PaymentRow>> {
        let pool = self.pool.clone();
        let (company, run, person) =
            (company.to_owned(), run.to_owned(), person.to_owned());
        Box::pin(async move {
            let row = sqlx::query(PAYMENT_SQL)
                .bind(company.as_str())
                .bind(run.as_str())
                .bind(person.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(PaymentRow {
                    status: text(&row, "status")?,
                    settled_at: option_text(&row, "settled_at")?,
                    batch_status: text(&row, "batch_status")?,
                })
            })
            .transpose()
        })
    }

    fn export_bytes<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> LaneReadFuture<'a, Option<ExportBytesRow>> {
        let pool = self.pool.clone();
        let (company, id) = (company.to_owned(), id.to_owned());
        Box::pin(async move {
            let row = sqlx::query(EXPORT_BYTES_SQL)
                .bind(id.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(ExportBytesRow {
                    export_html: text(&row, "export_html")?,
                    export_sha256: text(&row, "export_sha256")?,
                })
            })
            .transpose()
        })
    }

    fn country_pack<'a>(&'a self, id: &'a str) -> LaneReadFuture<'a, Option<CountryPackRow>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        Box::pin(async move {
            let row = sqlx::query(COUNTRY_PACK_SQL)
                .bind(id.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| pack_row(&row)).transpose()
        })
    }

    fn pay_group_facts<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> LaneReadFuture<'a, Option<PayGroupFacts>> {
        let pool = self.pool.clone();
        let (company, id) = (company.to_owned(), id.to_owned());
        Box::pin(async move {
            let row = sqlx::query(PAY_GROUP_FACTS_SQL)
                .bind(id.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(PayGroupFacts {
                    country_code: text(&row, "country_code")?,
                    currency: text(&row, "currency")?,
                    frequency: text(&row, "frequency")?,
                })
            })
            .transpose()
        })
    }

    fn statutory_run<'a>(&'a self, run: &'a str) -> LaneReadFuture<'a, Option<StatutoryRun>> {
        let pool = self.pool.clone();
        let run = run.to_owned();
        Box::pin(async move {
            let row = sqlx::query(STATUTORY_RUN_SQL)
                .bind(run.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| {
                Ok(StatutoryRun {
                    mode: text(&row, "mode")?,
                    country_pack_id: option_text(&row, "country_pack_id")?,
                    currency: text(&row, "currency")?,
                })
            })
            .transpose()
        })
    }

    fn statutory_items<'a>(
        &'a self,
        run: &'a str,
    ) -> LaneReadFuture<'a, Vec<PayrollItemTotals>> {
        let pool = self.pool.clone();
        let run = run.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(STATUTORY_ITEMS_SQL)
                .bind(run.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.iter()
                .map(|row| {
                    Ok(PayrollItemTotals {
                        gross_minor: integer(row, "gross_minor")?,
                        deductions_minor: integer(row, "deductions_minor")?,
                        net_minor: integer(row, "net_minor")?,
                        result_json: text(row, "result_json")?,
                    })
                })
                .collect()
        })
    }

    fn retained_pack<'a>(&'a self, id: &'a str) -> LaneReadFuture<'a, Option<CountryPackRow>> {
        let pool = self.pool.clone();
        let id = id.to_owned();
        Box::pin(async move {
            let row = sqlx::query(RETAINED_PACK_SQL)
                .bind(id.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|row| pack_row(&row)).transpose()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_oracles_are_pinned_and_distinct() {
        let results = [
            (ReadKind::PayGroups, r#"[{"id":"g1","name":"Monthly","currency":"THB","frequency":"monthly"}]"#),
            (ReadKind::People, r#"[{"id":"p1","legal_name":"Alice","email":null,"version":1}]"#),
            (ReadKind::ApproverName, r#"{"legal_name":"Alice"}"#),
            (ReadKind::RosterShift, r#"{"id":"s1","local_date":"2026-11-25","start_time":"09:00","end_time":"17:00","end_local":"2026-11-25T17:00","timezone":"Asia/Bangkok"}"#),
        ];
        let expected = [
            "c18b7d1ed74f6ff7dd6fd47d0ba34d11d6bbbb61fa17a29accaadcbec099de50",
            "8eacbb2f511143f0b6c808f8cd5401965277cc25ea587f7fd171f56e688665a7",
            "05faf8c3c0901abc4d238efaffa4d2472c8e79b6f87779db2bbf02bcf147ce16",
            "3c8b33376a170467ed04174fcf966714eaedd1da84414fecdbedf66c64017170",
        ];
        for ((kind, result), digest) in results.into_iter().zip(expected) {
            let sql = rendered_sql(kind);
            assert!(sql.starts_with("SELECT "), "owner SQL must be a read");
            assert!(!sql.contains('?'), "sqlx binds use $n");
            assert_eq!(oracle(kind, result), digest);
        }
    }

    #[test]
    fn payslip_sql_oracles_are_pinned_and_read_only() {
        let results = [
            (ReadKind::IssuableRun, r#"{"id":"run","input_digest":"d","revision":1,"version":2,"mode":"production","period_start":"2026-11-01","period_end":"2026-11-30","pay_date":"2026-11-30","currency":"THB"}"#),
            (ReadKind::IssuableItems, r#"[{"id":"item","engagement_id":"eng","person_id":"person","recipient_user_id":null,"input_snapshot_json":"{}","result_json":"{}","gross_minor":1000,"deductions_minor":10,"employer_cost_minor":5,"net_minor":990,"currency":"THB"}]"#),
            (ReadKind::OwnSlips, r#"[{"id":"slip","run_id":"run","run_revision":1,"pay_date":"2026-11-30","currency":"THB","gross_minor":1000,"deductions_minor":10,"net_minor":990,"source_digest":"s","export_sha256":"e","issued_at":100,"snapshot_json":"{}"}]"#),
            (ReadKind::Slip, r#"{"id":"slip","run_id":"run","person_id":"person","recipient_user_id":null,"snapshot_json":"{}","export_html":"<p>x</p>","export_sha256":"e","issued_at":100}"#),
            (ReadKind::Payment, r#"{"status":"settled","settled_at":null,"batch_status":"settled"}"#),
            (ReadKind::ExportBytes, r#"{"export_html":"<p>x</p>","export_sha256":"e"}"#),
        ];
        let expected = [
            "57deadb4524e929984ea4b8e0d78d9f5f7135a23db93236dfb32a7c0841d7b60",
            "f5dd9f5d9de1e60fd71c74baa970924fc399499306b8922f5f42fc7cd45dc672",
            "df05b73817752efff427660054934d706379af336bef1d71b77213fcddd95896",
            "1cd1e1f83af927aaa0c660002ebf46f868b400df2450f6d0e429d44066f3da2b",
            "beaf0b107aa606464f49dced37f6425ec5911b3556bf9af6c95b7e948de93325",
            "04156aa6abf68c18884eac593e0e4c77dbac8405b874f258207890f3446c4fe6",
        ];
        for ((kind, result), digest) in results.into_iter().zip(expected) {
            let sql = rendered_sql(kind);
            assert!(sql.starts_with("SELECT "), "owner SQL must be a read");
            assert!(!sql.contains('?'), "sqlx binds use $n");
            assert!(!sql.to_ascii_uppercase().contains("FOR UPDATE"));
            assert_eq!(oracle(kind, result), digest);
        }
    }

    #[test]
    fn statutory_sql_oracles_are_pinned_and_read_only() {
        let results = [
            (ReadKind::CountryPack, r#"{"id":"pack","country_code":"TH","version":1,"status":"active","rules_json":"{}","adapter_id":"thai","source_digest":"s","record_version":1}"#),
            (ReadKind::PayGroupFacts, r#"{"country_code":"TH","currency":"THB","frequency":"monthly"}"#),
            (ReadKind::StatutoryRun, r#"{"mode":"production","country_pack_id":null,"currency":"THB"}"#),
            (ReadKind::StatutoryItems, r#"[{"gross_minor":1000,"deductions_minor":10,"net_minor":990,"result_json":"{}"}]"#),
            (ReadKind::RetainedPack, r#"{"id":"pack","country_code":"TH","version":1,"status":"superseded","rules_json":"{}","adapter_id":"thai","source_digest":"s","record_version":2}"#),
        ];
        let expected = [
            "bb6ef86cdb64446e5279ca2a27666764a97e90caf5cd5ff6d134f23debed9e55",
            "fff4b81d6f54b74771b799cc335379e54fcd44cc115cb9683cff7654b9ea2668",
            "eff2ee208f800ccdec5843c32f4bcc7b58a3a7c892f9067dadf683d708979422",
            "d64573feccd8d1782a8e93a0646689f48c5a31ee93763ef8c3fefd2f298ad994",
            "1774740ccc5998695ed491769cd8626a1dc36c7d84bcbca6d6020bdc838ff5ac",
        ];
        for ((kind, result), digest) in results.into_iter().zip(expected) {
            let sql = rendered_sql(kind);
            assert!(sql.starts_with("SELECT "), "owner SQL must be a read");
            assert!(!sql.contains('?'), "sqlx binds use $n");
            assert!(!sql.to_ascii_uppercase().contains("FOR UPDATE"));
            assert_eq!(oracle(kind, result), digest);
        }
    }
}
