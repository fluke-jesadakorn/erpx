# Riverside Lantern pilot example

`pilot-example.json` is fictional THB setup data for the isolated hotel-and-restaurant browser demo. `bun run demo:prepare` creates Riverside Lantern as a second holding in disposable PostgreSQL databases, provisions eleven independent staff accounts, and configures PMS and POS through their owner commands. `bun run demo:destroy -- <run>` removes only that run. The fixture is not an accountant-approved tax setup or evidence of production readiness.

The planned acceptance journey starts with the front desk reserving Standard room 101 for the example guest. Housekeeping completes an arrival task and an independent supervisor inspects it. The front desk assigns room 101 and checks the guest in. A cashier opens a restaurant bill, kitchen staff prepare and deliver basil chicken rice and tea, and the cashier selects the checked-in stay for a full-bill room charge. The POS sale consumes ingredients, while PMS owns one folio charge and one revenue posting. At checkout, hotel cashier and Finance settle the folio, compare the external tax-document reference and bank evidence, and let the night auditor resolve blockers before close.

The following alternatives must be exercised in a resettable run with independent accounts:

| Case | Expected result |
| --- | --- |
| Wrong room or checked-out stay | No POS completion, stock consumption, folio entry, or revenue posting. |
| Replayed charge with the same key | The original result; exactly one folio source and posting. |
| Stale POS bill or hotel version | Conflict with the bill recoverable for review. |
| Closed folio or closing hotel day | Charge rejected without a partial transaction. |
| Revoked cashier | Charge denied immediately. |
| Bank-transfer reference without imported evidence | Pending payment; bill cannot complete. |
| Hotel transfer reference without imported evidence | Pending hotel request; no folio payment or bank posting, and Night Audit remains blocked. |
| Hotel transfer matched by a different Finance user to exact imported evidence | One PMS receipt, Finance payment and matched bank row; a retry returns the same result. |
| Imported bank row with a wrong amount, currency, account, date, or reference | No settlement or match. |
| Matched exact bank row, reviewed by another Finance user | One settled payment linked to one bank row; retry returns the same result. |
| Settled checkout and day close | The room night and restaurant charge appear once on the folio; the paid invoice, counted cash, balanced PMS journals, and next business date survive the Night Audit worker. |
| Night auditor access revoked after close starts | The worker blocks without advancing the day; a current administrator resumes it and the worker completes once. |
| Restaurant return or correction after room charge | Linked folio reversal and POS/stock treatment are implemented; independent multi-role business acceptance is still required. |

The embedded assistant may read the current PMS property/folio or POS sale only through the actor's existing owner API and role. The example accounts and records are fictional. Production AI remains disabled until the company explicitly chooses an AI provider. The assistant shows source versions and review steps, and never executes a room charge, payment, refund, or Night Audit action.

Demo account credentials and scenario guidance are written with private file permissions under `.runtime/browser/<run>/credentials.json` and `hospitality-guide.json`. The prepared hotel has an inspected room 101 and a checked-in fictional guest in room 202. Opening stock is disposable seed data, balanced by a posted opening inventory journal reviewed by a different Finance user. All email domains use `.example`, and `SANDBOX-*` references never represent real money or legal tax documents.
