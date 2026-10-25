---
status: proposed
---

# Canonical command receipt hash partitions

A missing-key canonical receipt lookup under SERIALIZABLE takes a predicate lock. Unrelated inserts into the same small receipt index can form a serialization cycle across independent Property transactions. A two-connection owned diagnostic reproduces `40001`; placing different keys in separate hash partitions lets both transactions commit. This diagnosis does not itself establish the full workload performance target.

Migration 1040 keeps `command_receipts` as the canonical SQL table, partitioned by `command_id` into 32 hash partitions. Its primary key includes the partition key, so each command still has one globally unique receipt. Keep canonical receipt lookup, current replay authority, envelope hashes, atomic audit/outbox/money effects, SERIALIZABLE isolation, Property locks and bounded retries unchanged. Hash collisions and real business conflicts still require the existing retry behavior.

Retain the complete pre-upgrade table as `command_receipts_before_partition`, copy every field into the new canonical parent, and rebind all referencing foreign keys with their original definitions. Preserve indexes, constraints, table/column permissions, ownership and user triggers. Unsupported row-security, dependent views, outbound foreign keys or security labels stop the migration atomically for review. Exclude physical receipt metadata from work-cache invalidation; do not reset existing revisions. The retained baseline is migration evidence and never the live replay source.

The populated upgrade/replay test compares original rows, evidence references, foreign keys, permissions and cache revisions; independent and same-key SERIALIZABLE cases check commit/uniqueness/rollback. Full-snapshot restore removes partition roots only after checksum/scope checks, a protected backup and service pause; the archive recreates every foreign key and index with exit-on-error retained. PostgreSQL 18.6 clean restore otherwise tries to remove inherited child primary keys first. Native encrypted restore, compatible rollback, corrupted-backup refusal and source-linked recovery pass; final compiled restore, full native, capacity and browser acceptance remain pending. The [production plan](../production-plan.md) remains the sole active checklist.

PostgreSQL documents [hash partitions and partition-key uniqueness](https://www.postgresql.org/docs/18/ddl-partitioning.html) and [SERIALIZABLE predicate locks and possible false-positive failures](https://www.postgresql.org/docs/18/transaction-iso.html).
