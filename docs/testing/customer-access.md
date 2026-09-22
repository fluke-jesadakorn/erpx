# Customer authority — source-bound access

Customer access is an explicit, expiring binding between one approved quotation or sales order and one verified email address. A different, currently eligible sales manager must review the request before the customer projection becomes readable.

## Staff workflow

- Start from an approved customer quotation or sales order, record the recipient email, expiry and authority evidence.
- The request appears in Work and Inbox for an eligible independent sales manager. The requester cannot approve their own request.
- Approval checks the source again. Draft, cancelled, moved, stale or out-of-scope documents cannot be published.
- The requester or an authorized sales manager can revoke pending or approved access immediately. Events and customer reads remain immutable.
- Company, Branch and Department scope apply to the staff queue, detail, counts and audit view.

## Customer projection

The customer signs in with a verified email. The first account to read an approved binding becomes its explicit identity; another account with the same email cannot reuse it. Each view records binding version, user, time and projection version in the same statement that verifies current approval and expiry.

The `customer-order-v1` allowlist contains only order number, kind, state, due date, currency and recorded fulfillment references. Internal notes, staff events, costs, HR, Legal, payment details and private Support evidence are excluded.

## Customer actions

Each projected order exposes two source-bound actions: the customer can acknowledge delivery or request support with an optional 500-character note. The command rechecks the approved binding, expiry, expected binding version, verified identity and publishable source state in a serializable transaction. A replayed `Idempotency-Key` returns the original result; a different request using the same key is rejected. Each action is retained as immutable evidence, written to the audit and outbox streams, and creates actionable `commerce_order` notifications for scoped staff with `commerce.read`.

## Remaining scope

This implementation does not cover duplicate-profile merge, identity-change review, communication consent/preferences, delegated customer administrators, multi-source grants, customer replies/uploads, email delivery, retention/export, provider acceptance or real customer UAT.
