-- Reporting-chain approval also supports company/branch staff without a department.
-- Current placement and participant eligibility are checked by the spending route.
ALTER TABLE finance_expenses DROP CONSTRAINT finance_expenses_check;
ALTER TABLE finance_expenses ADD CONSTRAINT finance_expenses_check
  CHECK(workflow_version=1 OR status IN ('draft','rejected') OR
    (claimant_user_id IS NOT NULL AND employee_id IS NOT NULL AND engagement_id IS NOT NULL
    AND expense_date IS NOT NULL AND length(merchant)>=2
    AND currency IS NOT NULL AND amount_minor IS NOT NULL AND category_account_id IS NOT NULL
    AND length(business_purpose)>=5 AND length(receipt_reference)>=2));
