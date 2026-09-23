# Hotel transfer — isolated acceptance evidence

The disposable Riverside Lantern holding uses fictional THB data and independent hotel cashier and Finance approver accounts. In the actual PMS folio browser screen, the cashier opened a shift and recorded a transfer reference. The request stayed pending without a folio payment. The test inserted a fictional imported statement row into that run's isolated database, then a different Finance user selected it in the PMS browser screen. The request, folio receipt, Finance payment and matched bank row were linked and remained visible after reload. The full seven-scenario desktop suite passed (21.8 seconds) on run `c1b6053f6958`.

The integration test rejects self-verification and bank rows with the wrong amount. The expanded suite checks reference, currency, date and bank-account mismatches. A pending transfer appears in the independent Finance approver's Work queue, but not in the cashier's queue. A cancelled request leaves no folio or ledger payment; replay uses the original command result. Night Audit identifies a pending hotel transfer as a blocker.

The browser test seeds an imported bank row inside a disposable database. It does not establish the bank-import UI journey, real bank/provider evidence, a signed hotel/Finance business decision, or production acceptance. Non-cash PMS refunds remain gated.
