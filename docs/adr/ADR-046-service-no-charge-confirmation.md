# Finance confirmation of a fully warranted Service visit

A fully warranted Service visit already retains a real billing release, Invoice identity and zero-valued money captures, but the Invoice cannot legitimately post a zero journal or receive a payment. Finance records an independent **Service No-Charge Confirmation**, with a distinct `no_charge_confirmed` terminal state and an immutable decision tied to that original handoff. Treating it as paid or voided would misstate the outcome; retaining a draft with a separate terminal projection would leave every owner to reinterpret an otherwise unfinished Invoice.

Only the exact fully covered Service source qualifies. The confirmation retains Company, source identities, captured pricing and stock/cost history; ordinary nonzero posting, payment, cancellation and refund rules continue to apply to their own outcomes. Independence is checked at the decision and before commit, while later lawful identity linking retains the original decision's historical evidence.

This is an additive development implementation decision. Its new schema and owner workflow require isolated native and populated migration verification before installation; the decision does not certify product acceptance or authorize Commercial withdrawal without all other owners' obligations being settled.
