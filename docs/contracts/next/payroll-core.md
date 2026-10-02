# Payroll core and controlled corrections

The payroll core is a Company-scoped gross-pay preparation workflow. It snapshots approved compensation, approved time, payroll handoffs and approved payroll-advance repayments with the pay-group proration and overtime policy, then produces a versioned preview. It does not calculate Thai statutory deductions: `createPayrollRun` rejects `mode=production` with `PAYROLL_ENGINE_UNAVAILABLE`. Country-pack records alone do not install a calculation adapter.

Each Position may have a versioned monthly pay default with pay group and currency. People and Organization show its amount only to authorized compensation readers. When HR accepts a recommendation for an Engagement without compensation history, the system creates a draft Compensation Revision; a different person must approve it. Existing approved Compensation Revisions remain the sole pay basis for Payroll. A Position default change does not change current pay or rewrite past revisions. Acme development amounts are fictional USD fixtures, not certified compensation or Thai statutory rules.

`POST /api/v1/holdings/{holdingSlug}/people/organization/positions/{positionId}/default-pay` creates the next version using a matching active monthly pay group, amount in minor units, and expected version. People directory and lifecycle placement requests carry explicit `acceptRecommendedPay`; no pay amount is returned to a caller without compensation permission.

Controlled Finance GL posting and payment-batch preparation/approval/export/result confirmation exist for approved core runs (`postPayroll` and payment services in `src/server/people-payroll.ts`). They do not certify Thai gross-to-net or execute a bank transfer. Production rollout still closes payroll processing until the required adapter and acceptance are delivered. The [production plan](../../production-plan.md) requires Thai payroll before the whole hotel release opens; it is no longer deferred from the delivery scope.

## Review and correction lifecycle

1. A holder of `payroll.prepare` creates and calculates the run.
2. The named independent holder of `payroll.approve` approves the current preview. The input digest and review export identify the exact revision reviewed.
3. A preparer can request a correction only against the current approved version. The request snapshots the run version, revision, input digest, totals and reason.
4. A different person with active Company-level `payroll.approve` authority approves or rejects the request. The requester can cancel a pending request. Stale decisions fail if the run evidence changed.
5. Approval returns the run to `calculating`, clears its prior approver and increments its version. Recalculation creates a new revision; previous exports remain downloadable and unchanged.

Only one correction can be pending for a run. Request and decision commands require an idempotency key and expected version. Correction events are immutable. Inbox visibility is recalculated from the recipient's current Company-level payroll authority, so revocation removes access to the target even if an old notification remains stored.

## Routes

| Route | Command |
| --- | --- |
| `POST /api/v1/holdings/{holdingSlug}/people/payroll/runs/{runId}/corrections` | Request correction of the current approved run version |
| `POST /api/v1/holdings/{holdingSlug}/people/payroll/corrections/{correctionId}/decide` | Independently approve/reject, or requester-cancel, a pending correction |

The run detail at `/holdings/{holdingSlug}/people/payroll/runs/{runId}` shows correction history, source revision, reasons and role-appropriate actions.

## Final pay from retirement clearance

The assigned Payroll owner can prepare final pay directly from the retirement clearance case. The command verifies the task owner, task version, engagement pay group/currency and last working date, then creates one `final_pay` preview scoped to that engagement. The run is linked to the clearance task and records an immutable preparation event. It must be calculated and reviewed through the regular Payroll controls before the clearance task can cite it as source evidence. A second active preparation for the same task is rejected. This preparation still does not post statutory payroll or pay a bank file.

`POST /api/v1/holdings/{holdingSlug}/people/lifecycle/tasks/{taskId}/final-pay` creates the linked person-scoped preview and requires `Idempotency-Key` plus the current task version.

## Payroll advances

Employees with `self.requests` can request an advance for their own active pay-group engagement; Payroll preparation roles can submit one for an eligible employee. The request records amount, currency, reason, first deduction period and one-to-24 installments. A different Company-level `payroll.approve` holder must approve or reject it, while the requester can cancel a pending request. Approval creates an immutable decision event and an exact repayment schedule whose remainder is distributed to the earliest installments.

During calculation, open or reserved repayments whose due period overlaps the run are included as deduction components and copied into the input snapshot. Each repayment is guarded by its advance and repayment version, so stale or cancelled schedules cannot silently change a reviewed preview. The advance list and request/review actions are available at `/holdings/{holdingSlug}/people/payroll/advances`; API commands are `POST /api/v1/holdings/{holdingSlug}/people/payroll/advances` and `POST /api/v1/holdings/{holdingSlug}/people/payroll/advances/{advanceId}/decide`.
