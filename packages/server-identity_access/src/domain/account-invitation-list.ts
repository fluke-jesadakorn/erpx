export type AccountInvitationListActor = {
  role: string;
  holdingId: string;
  companyId: string;
  branchId: string | null;
};

export type AccountInvitationListScope = Pick<AccountInvitationListActor, "holdingId" | "companyId" | "branchId">;

export type AccountInvitationListItem = {
  id: string;
  person_id: string;
  email: string;
  status: string;
  expires_at: number;
  delivery_state: string | null;
  attempts: number | null;
  error: string | null;
  sent_at: number | null;
};

/** The activation panel has always been restricted to the Holding owner. */
export function accountInvitationListScope(actor: AccountInvitationListActor): AccountInvitationListScope | null {
  return actor.role === "holding_owner"
    ? { holdingId: actor.holdingId, companyId: actor.companyId, branchId: actor.branchId }
    : null;
}
