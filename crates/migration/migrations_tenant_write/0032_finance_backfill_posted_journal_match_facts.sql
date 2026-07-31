-- Backfill match facts for legacy posted journals only when historical
-- payload evidence is complete. The chart's current currency is not a
-- historical fact and must never be used to rewrite an old journal.
--
-- This is a one-time tenant-write migration. The migration role owns the
-- tenant table, so it temporarily removes FORCE RLS for the bounded upgrade
-- transaction while the runtime policy remains enabled and is restored below.
-- Runtime repositories continue to require erpx.holding_ref through RLS.
-- The exception handler also restores the named trigger and FORCE RLS when a
-- migration runner executes this file outside an enclosing transaction.
DO $migration$
BEGIN
    ALTER TABLE erpx_tenant.finance_posted_journals
        NO FORCE ROW LEVEL SECURITY;
    ALTER TABLE erpx_tenant.finance_posted_journals
        DISABLE TRIGGER finance_posted_journals_immutable;

    WITH source_journals AS (
        SELECT
            journal_id,
            payload #>> '{entry,currency}' AS source_currency,
            CASE
                WHEN jsonb_typeof(payload #> '{entry,lines}') = 'array'
                    THEN payload #> '{entry,lines}'
                ELSE '[]'::jsonb
            END AS lines
        FROM erpx_tenant.finance_posted_journals
        WHERE currency IS NULL OR total_minor IS NULL
    ), line_values AS (
        SELECT
            source.journal_id,
            source.source_currency,
            line.value,
            CASE
                WHEN line.value IS NULL OR NOT (line.value ? 'debit_minor')
                    THEN 0::NUMERIC
                WHEN line.value->>'debit_minor' ~ '^[0-9]{1,19}$'
                    THEN (line.value->>'debit_minor')::NUMERIC
                ELSE NULL::NUMERIC
            END AS debit_minor,
            CASE
                WHEN line.value IS NULL OR NOT (line.value ? 'credit_minor')
                    THEN 0::NUMERIC
                WHEN line.value->>'credit_minor' ~ '^[0-9]{1,19}$'
                    THEN (line.value->>'credit_minor')::NUMERIC
                ELSE NULL::NUMERIC
            END AS credit_minor
        FROM source_journals AS source
        LEFT JOIN LATERAL jsonb_array_elements(source.lines) AS line(value)
            ON TRUE
    ), derived_facts AS (
        SELECT
            journal_id,
            MAX(source_currency) AS source_currency,
            COUNT(value) AS line_count,
            COUNT(*) FILTER (
                WHERE debit_minor IS NOT NULL
                  AND credit_minor IS NOT NULL
                  AND (
                      (debit_minor > 0 AND credit_minor = 0)
                      OR (debit_minor = 0 AND credit_minor > 0)
                  )
            ) AS valid_line_count,
            SUM(debit_minor) AS total_numeric,
            SUM(credit_minor) AS credit_numeric
        FROM line_values
        GROUP BY journal_id
    )
    UPDATE erpx_tenant.finance_posted_journals AS journal
    SET currency = CASE
            WHEN journal.currency IS NULL
                 AND facts.source_currency ~ '^[A-Z]{3}$'
                THEN facts.source_currency
            ELSE journal.currency
        END,
        total_minor = CASE
            WHEN journal.total_minor IS NULL
                 AND facts.line_count > 0
                 AND facts.line_count = facts.valid_line_count
                 AND facts.total_numeric = facts.credit_numeric
                 AND facts.total_numeric BETWEEN 1 AND 9223372036854775807
                THEN facts.total_numeric::BIGINT
            ELSE journal.total_minor
        END
    FROM derived_facts AS facts
    WHERE journal.journal_id = facts.journal_id
      AND (
          (journal.currency IS NULL AND facts.source_currency ~ '^[A-Z]{3}$')
          OR (
              journal.total_minor IS NULL
              AND facts.line_count > 0
              AND facts.line_count = facts.valid_line_count
              AND facts.total_numeric = facts.credit_numeric
              AND facts.total_numeric BETWEEN 1 AND 9223372036854775807
          )
      );

    ALTER TABLE erpx_tenant.finance_posted_journals
        ENABLE TRIGGER finance_posted_journals_immutable;
    ALTER TABLE erpx_tenant.finance_posted_journals
        FORCE ROW LEVEL SECURITY;
EXCEPTION WHEN OTHERS THEN
    BEGIN
        ALTER TABLE erpx_tenant.finance_posted_journals
            ENABLE TRIGGER finance_posted_journals_immutable;
    EXCEPTION WHEN OTHERS THEN
        NULL;
    END;
    BEGIN
        ALTER TABLE erpx_tenant.finance_posted_journals
            FORCE ROW LEVEL SECURITY;
    EXCEPTION WHEN OTHERS THEN
        NULL;
    END;
    RAISE;
END;
$migration$;
