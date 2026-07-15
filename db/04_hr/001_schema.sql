-- folio/db/04_hr/001_schema.sql
-- HR schema (employees, leave_requests, user_sessions).
-- Schema-qualify everything with hr.*.
-- Apply to: folio_db with search_path including hr schema.
-- Idempotent: safe to re-run.

BEGIN;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE IF NOT EXISTS hr.employees (
    id                    UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    employee_code         TEXT UNIQUE NOT NULL,
    line_user_id          TEXT UNIQUE,
    name                  TEXT NOT NULL,
    department            TEXT NOT NULL,
    position              TEXT NOT NULL,
    role                  TEXT NOT NULL DEFAULT 'staff',
    job_description       TEXT NOT NULL,
    total_sick_leave      INT DEFAULT 30,
    used_sick_leave       INT DEFAULT 0,
    total_annual_leave    INT DEFAULT 10,
    used_annual_leave     INT DEFAULT 0,
    total_personal_leave  INT DEFAULT 6,
    used_personal_leave   INT DEFAULT 0,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS hr.leave_requests (
    id             UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    employee_id    UUID NOT NULL REFERENCES hr.employees(id) ON DELETE CASCADE,
    leave_type     TEXT NOT NULL,
    start_date     DATE NOT NULL,
    end_date       DATE NOT NULL,
    days           NUMERIC(3, 1) NOT NULL,
    reason         TEXT,
    reject_reason  TEXT,
    status         TEXT NOT NULL DEFAULT 'pending',
    approved_by    UUID REFERENCES hr.employees(id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS hr.user_sessions (
    line_user_id   TEXT PRIMARY KEY,
    current_state  TEXT NOT NULL DEFAULT 'idle',
    temp_data      JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_employees_line_user ON hr.employees (line_user_id);
CREATE INDEX IF NOT EXISTS idx_leave_requests_employee ON hr.leave_requests (employee_id);
CREATE INDEX IF NOT EXISTS idx_leave_requests_status ON hr.leave_requests (status);
CREATE INDEX IF NOT EXISTS idx_user_sessions_state ON hr.user_sessions (current_state);

COMMIT;
