# Thai Payroll acceptance fixtures

These files are **synthetic engineering examples** created for repeatable development tests. They are not Thai statutory rates, expert-approved expected results, or an approved bank format. The production calculation and bank-file release gates remain closed until Payroll, Finance, and the bank owner sign off on separately sourced evidence.

- `synthetic-kernel-v1.json` supplies fictional effective rules and hand-calculated expected results for the deterministic calculation kernel.
- `synthetic-bank-result-v1.csv` exercises one confirmed and one failed payment row. The confirmed row's transaction ID is an example only. The payment service still requires an existing imported bank statement transaction with the exact amount, account, currency, date, external reference, and current version before it can settle a liability.

The synthetic result CSV has this exact header:

```csv
reference,status,confirmation_ref,bank_transaction_id,bank_version
```

`confirmed` rows require all three evidence fields. `failed` rows leave them empty and retain the unpaid liability. References and bank transaction IDs must be unique within a file. A bank-specific adapter needs an approved layout and provenance check before a real bank response can use this path.

## Evidence needed before production activation

1. Payroll expert signs rule versions, effective dates, rounding policy, tax deductions, social-security eligibility, and expected gross-to-net results for monthly, daily, hourly, overtime, bonus, service charge, part-period, prior-year/YTD correction, foreign employee, advance, and final-pay cases.
2. Finance approves the account mapping for wages, employer social security expense, employee/employer social security payable, withholding payable, employee net payable, advances, benefits, service charge, and bank settlement; verify open and closed period adjustments and branch/department totals.
3. Bank owner supplies real export and return-file specifications plus accepted and rejected sample files. Treasury and Finance verify references, matching imported statement rows, duplicates, partial rejection, replay, and bank evidence retention.
4. HR, an independent approver, Finance, and Treasury execute a full Thai and English desktop/mobile round on staging and record their acceptance.
