-- 2026-07-07-A: book bank slip — second slip row per expense for transfer payees.
--
-- Refactor: tier 1 step A. A 'book_bank' slip carries the OCR'd bank account
-- (bank_name, account_number, account_name) so a transfer-expense can be
-- reimbursed into the right payee account. Two slips per expense, both linked
-- to the same expense_id, both satisfy the existing
-- `slips_check_exactly_one_parent` deferred trigger (each slip has exactly
-- one parent; the pair just happens to share it).
--
-- Column choices:
--   * `kind`         — distinguishes receipt vs. book_bank rows. Existing rows
--                      backfill 'receipt'. New uploads specify the kind.
--   * bank_*         — the extracted fields, nullable for receipt rows.
--
-- CHECK constraint keeps kind ∈ {receipt, book_bank} to fail fast on
-- unknown values.
--
-- The deferred exactly-one-parent trigger from 2026-07-02-C and the
-- pending-aware relaxation from 2026-07-03-A continue to apply unchanged.

BEGIN;

-- 1. Add columns ----------------------------------------------------
ALTER TABLE slips
  ADD COLUMN IF NOT EXISTS kind           VARCHAR(20) NOT NULL DEFAULT 'receipt',
  ADD COLUMN IF NOT EXISTS bank_name      VARCHAR(150),
  ADD COLUMN IF NOT EXISTS account_number VARCHAR(30),
  ADD COLUMN IF NOT EXISTS account_name   VARCHAR(150);

-- 2. Backfill kind='receipt' (DEFAULT already covers it, but make it explicit
--    for rows that were inserted before DEFAULT applied) ---------------
UPDATE slips SET kind = 'receipt' WHERE kind IS NULL;

-- 3. CHECK constraint -----------------------------------------------
ALTER TABLE slips
  DROP CONSTRAINT IF EXISTS slips_kind_chk;
ALTER TABLE slips
  ADD CONSTRAINT slips_kind_chk CHECK (kind IN ('receipt','book_bank'));

-- 4. Index on kind for filtering book-bank slips on an expense -------
CREATE INDEX IF NOT EXISTS idx_slips_kind_expense
  ON slips(kind, expense_id)
  WHERE expense_id IS NOT NULL;

-- 5. Comments --------------------------------------------------------
COMMENT ON COLUMN slips.kind IS
  'receipt = the typical receipt/ใบเสร็จ slip (default). '
  'book_bank = passbook image for a transfer payee — carries bank_name / '
  'account_number / account_name. Two slips per expense are permitted; each '
  'row has exactly one parent per the slips_exactly_one_parent trigger.';
COMMENT ON COLUMN slips.bank_name IS
  'For book_bank slips: issuing bank name (free text or one of '
  'Krungthai/SCB/Bangkok Bank/Kasikorn/TMBThanachai/Other).';
COMMENT ON COLUMN slips.account_number IS
  'For book_bank slips: payee bank account number (digits, no dashes).';
COMMENT ON COLUMN slips.account_name IS
  'For book_bank slips: payee name as printed on the passbook.';

COMMIT;

-- Verify -------------------------------------------------------------
SELECT kind, COUNT(*) FROM slips GROUP BY kind ORDER BY kind;
