# Customer identity changes

Changes to customer legal name, tax registration, phone and structured address use a reviewed before/after snapshot instead of directly overwriting the master.

- The requester selects an active customer and records the current customer version, proposed values and evidence.
- A different, currently eligible Company sales manager applies or rejects the proposal from Work/Inbox.
- Apply succeeds only if the customer master still has the captured version; a concurrent edit forces a fresh review.
- Rejected work can be corrected and resubmitted against the current customer version without erasing earlier evidence.
- Apply updates the customer master and the request/event history in one transaction. Source identity and all events are immutable.

This increment covers legal name, tax registration, phone, structured address, notification email and explicit notification consent. The proposed values are captured in the before/after snapshot, require independent review, and apply atomically only when the customer version is unchanged. External registry verification, browser acceptance and real privacy/business sign-off remain.
