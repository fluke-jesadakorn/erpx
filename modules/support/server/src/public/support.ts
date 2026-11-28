/** Compatibility entry; its legacy implementation is tracked in ownership.json. */
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SupportRow } from "@erpx/support-server/application/command";

/** Type-only compatibility for downstream `typeof` projections; no runtime TypeScript reader remains. */
export type SupportWorkspaceData = {
  items: SupportRow[];
  sources: SupportRow[];
  canCreate: boolean;
  canCreateOrder: boolean;
  ownReportsOnly: boolean;
};
export type SupportRecipient = { id: string; name: string };
export declare const supportWorkspace: (a: WorkContext) => Promise<SupportWorkspaceData>;
export declare const supportRecipients: (a: WorkContext, orderId: string) => Promise<SupportRecipient[]>;

export { supportCreateSchema, supportChangeSchema, supportTicketSourceGuards, getSupportTicket, supportWorkspacePurchaseAccess, supportDetail, supportWork } from "@erpx/support-server/adapters/support";
