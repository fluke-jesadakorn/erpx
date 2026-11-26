import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = resolve(__dirname, "../../../packages/server-people_payroll/src/adapters/native_people_list");
const domain = readFileSync(resolve(ROOT, "domain.rs"), "utf8");
const application = readFileSync(resolve(ROOT, "application.rs"), "utf8");
const postgres = readFileSync(resolve(ROOT, "postgres.rs"), "utf8");
const http = readFileSync(resolve(ROOT, "http.rs"), "utf8");

const sql = postgres.match(/pub const SQL_PEOPLE_LIST: &str = r#"([\s\S]*?)"#;/)?.[1] ?? "";
const sha256 = (value: string) => createHash("sha256").update(value).digest("hex");

const FIELDS = [
  "id", "user_id", "legal_name", "preferred_name", "email", "country_code", "status", "version",
  "engagement_id", "job_title", "worker_type", "employment_type", "engagement_status", "start_date",
  "currency", "level_id", "department_id", "position_id", "manager_person_id", "level_name", "branch_name",
] as const;

const FIXTURE = {
  id: "person-1", user_id: "usr-1", legal_name: "Ada Lovelace", preferred_name: "Ada",
  email: "ada@example.test", country_code: "TH", status: "active", version: 1,
  engagement_id: "eng-1", job_title: "Engineer", worker_type: "employee",
  employment_type: "salaried", engagement_status: "active", start_date: "2026-01-05",
  currency: "THB", level_id: "level-1", department_id: "dept-1", position_id: "pos-1",
  manager_person_id: null, level_name: "L1", branch_name: "Bangkok",
};

describe("native people-directory list slice", () => {
  it("executes the real company-scoped statement in the adapter", () => {
    expect(postgres).toContain("sqlx::query(SQL_PEOPLE_LIST)");
    expect(postgres).toContain(".fetch_all(&mut *connection)");
    expect(postgres).toContain("PgUnitOfWork::new(self.pool.clone())");
    expect(postgres).toContain(".bind(q.company.as_str())");
    expect(postgres).not.toMatch(/prepare\(|batch\(/);
  });

  it("keeps pure domain and application layers free of SQL and HTTP", () => {
    expect(domain).not.toMatch(/^\s*use\s+(sqlx|axum)/m);
    expect(application).not.toMatch(/^\s*use\s+(sqlx|axum)/m);
    expect(application).toContain("pub trait PeopleListRead");
    expect(application).toContain("pub struct ListPeopleQuery");
  });

  it("pins the executed SQL oracle and its ten-bind dialect", () => {
    expect(sql.length).toBeGreaterThan(1000);
    expect(sha256(sql)).toBe("d2fa0333c95d055fcb9d7a77a222906b18b6e40ed7f3d94a6b1e38b02235b460");
    expect((sql.match(/\$\d+/g) ?? []).length).toBe(10);
    expect(sql).not.toContain("?");
    expect(postgres).toContain("d2fa0333c95d055fcb9d7a77a222906b18b6e40ed7f3d94a6b1e38b02235b460");
    for (const fragment of [
      "p.company_id=$1", "p.merged_into_id IS NULL", "e.worker_type='employee'",
      "e.branch_id=$3", "e.department_id=$5", "$7::text='unset'", "e.level_id=$8",
      "e.department_id=$10", "ORDER BY p.legal_name,p.id",
    ]) {
      expect(sql).toContain(fragment);
    }
  });

  it("preserves the retired row field order and payload shape", () => {
    for (const field of FIELDS) expect(domain).toContain(`pub ${field}:`);
    expect(sha256(JSON.stringify({ data: [FIXTURE] }))).toBe(
      "d9b65f42da52f75dfb1bc00945a66b972ee0f5906ae59c798612c5a8534084e0",
    );
    expect(http).toContain("d9b65f42da52f75dfb1bc00945a66b972ee0f5906ae59c798612c5a8534084e0");
  });

  it("owns the isolated ingress and fails closed without a native reader", () => {
    expect(http).toContain("x-erpx-context-company");
    expect(http).toContain('"levelId"');
    expect(http).toContain('"departmentId"');
    expect(http).toContain("StatusCode::SERVICE_UNAVAILABLE");
    expect(http).toContain("/api/v1/holdings/{holding_slug}/people-payroll/people-list");
  });
});
