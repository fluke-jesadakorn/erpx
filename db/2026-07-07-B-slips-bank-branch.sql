-- 2026-07-07-B: optional branch column on book_bank slips.
--
-- Adds slips.bank_branch (nullable VARCHAR(150)) so the payee branch as
-- printed on the passbook can be captured. Optional — book_bank slips
-- already exist with NULL, and the OCR pipeline + confirm action treat
-- the field as free text; not part of bankConfidenceScore.

BEGIN;

ALTER TABLE slips
  ADD COLUMN IF NOT EXISTS bank_branch VARCHAR(150);

COMMENT ON COLUMN slips.bank_branch IS
  'For book_bank slips: branch as printed on the passbook, e.g. '
  '''0080 สาขาฟิวเจอร์พาร์ค รังสิต''. Free text. Optional.';

COMMIT;

-- Verify -------------------------------------------------------------
SELECT column_name, data_type, is_nullable
  FROM information_schema.columns
 WHERE table_name = 'slips' AND column_name = 'bank_branch';