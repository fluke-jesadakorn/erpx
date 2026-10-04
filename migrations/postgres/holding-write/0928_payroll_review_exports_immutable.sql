-- Preserve the exact issued revision, object key and digest across payroll
-- corrections. Historical export rows retain their original meaning and bytes.
CREATE FUNCTION hr_payroll_review_export_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION 'PAYROLL_REVIEW_EXPORT_IMMUTABLE' USING ERRCODE='23514';
END $$;
CREATE TRIGGER hr_payroll_review_export_immutable BEFORE UPDATE OR DELETE ON hr_payroll_review_exports
 FOR EACH ROW EXECUTE FUNCTION hr_payroll_review_export_immutable();
