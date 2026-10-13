/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
export { claimJob, runOne, activeSlots, directoryWork, posQueueWork, scheduledWork, maintenance, expenseAiWork, billingWork, attachmentWork, pmsWork, legalWork, supportWork, customerNotificationWork } from "@/server/modules/runtime/adapters/runtime/jobs";
export type { PendingEvent } from "@/server/modules/runtime/adapters/runtime/jobs";
