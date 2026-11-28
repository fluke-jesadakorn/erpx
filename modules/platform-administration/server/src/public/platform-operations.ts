/** Presentation contract for the native Platform Administration operations
 * projection. Authority and persistence live in the Rust owner; these types
 * only describe the payload shared with Platform UI components. */
export type PlatformDeadLetter = {
  id: string;
  eventType: string;
  aggregateId: string;
  aggregateVersion: number;
  attempts: number;
  lastError: string | null;
  deadLetterAt: number;
};
export type PlatformHoldingOperation = {
  id: string;
  slug: string;
  displayName: string;
  status: "healthy" | "pending" | "attention" | "unavailable";
  pendingEvents: number;
  deadLetterEvents: number;
  claimedEvents: number;
  oldestPendingAt: number | null;
  jobHeartbeatAt: number | null;
  error: string | null;
  deadLetters: PlatformDeadLetter[];
  migrations: {
    track: "holding-write";
    applied: number;
    pending: string[];
    latestApplied: string | null;
    error: string | null;
  };
};
export type PlatformOperationsSummary = {
  generatedAt: number;
  actorRole: string;
  controlHealth: { service: string; heartbeatAt: number; details: string }[];
  holdings: PlatformHoldingOperation[];
  totals: {
    holdings: number;
    pendingEvents: number;
    deadLetterEvents: number;
    claimedEvents: number;
  };
};
export type PlatformProvisioningRow = {
  id: string;
  holdingId: string;
  slug: string;
  displayName: string;
  writeBinding: string;
  readBinding: string;
  filesBinding: string;
  status: "provisioning" | "active" | "failed" | "suspended";
  requestedBy: string;
  reason: string;
  error: string | null;
  version: number;
  requestedAt: number;
  activatedAt: number | null;
  updatedAt: number;
};
export type PlatformProvisioningSnapshot = {
  rows: PlatformProvisioningRow[];
  availableSlots: {
    id: string;
    write: string;
    read: string;
    files: string;
    beta?: boolean;
    internal?: boolean;
  }[];
};
export type ProvisionHoldingResult = {
  id: string;
  holdingId: string;
  slug: string;
  displayName: string;
  status: "active";
  version: number;
  activatedAt: number;
};
