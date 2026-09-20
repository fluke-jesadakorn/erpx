# Customer identity changes

Changes to customer legal name and tax registration use a reviewed before/after snapshot instead of directly overwriting the master.

- The requester selects an active customer and records the current customer version, proposed values and evidence.
- A different, currently eligible Company sales manager applies or rejects the proposal from Work/Inbox.
- Apply succeeds only if the customer master still has the captured version; a concurrent edit forces a fresh review.
- Rejected work can be corrected and resubmitted against the current customer version without erasing earlier evidence.
- Apply updates the customer master and the request/event history in one transaction. Source identity and all events are immutable.

This increment covers legal name and tax registration only. Address/contact identity, consent, duplicate transactional merge, external registry verification, browser acceptance and real privacy/business sign-off remain.
