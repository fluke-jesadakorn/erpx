# Company scoped staff Inbox events and deliveries

ERPX stores one immutable Inbox Event per meaningful source occurrence and one Inbox Delivery per recipient. Events carry a stable deduplication key, source reference, category, and safe preview; deliveries own independent read and archive state. This separates multi-recipient fanout from personal Inbox state, allows retries without duplicate messages, and prevents a sender from treating a recipient's read state as workflow acknowledgement. Current source access is checked on every read, so a delivery never grants access to the underlying record.

Existing `staff_notifications` writes are bridged to this model during cutover, preserving their IDs and read timestamps while old endpoints remain available. A bounded durable worker fans out broadcasts and sends calendar reminders. The staff Inbox is inside ERPX; external email, push, and platform administrator Inbox are outside this decision.
