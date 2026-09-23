# Riverside Lantern pilot example

`pilot-example.json` is fictional THB setup data for an isolated hotel-and-restaurant demo. It is a preparation fixture, not an activated company, an accountant-approved tax setup, or evidence of a successful production workflow. The example addresses the user's request to generate sample pilot inputs while a real pilot property has not been supplied.

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
| Imported bank row with a wrong amount, currency, account, date, or reference | No settlement or match. |
| Matched exact bank row, reviewed by another Finance user | One settled payment linked to one bank row; retry returns the same result. |
| Restaurant return or correction after room charge | Linked folio reversal and POS/stock treatment still require implementation and acceptance. |

No demo account is created from the listed email addresses until the isolated demo reset and provisioning workflow is implemented. All email domains use `.example`, and `SANDBOX-*` references never represent real money or legal tax documents.
