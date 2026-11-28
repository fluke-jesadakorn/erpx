import "server-only";
import { supportTicketsProjection } from "./final-root-owner-projection-sources";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  support_tickets: supportTicketsProjection,
} as const;
