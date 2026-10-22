import { accountInvitationListScope, type AccountInvitationListActor, type AccountInvitationListItem, type AccountInvitationListScope } from "@erpx/server-identity_access/domain/account-invitation-list";

export interface AccountInvitationListRepository {
  list(scope: AccountInvitationListScope): Promise<AccountInvitationListItem[]>;
}

export function createAccountInvitationListApplication(repository: AccountInvitationListRepository) {
  return {
    async read(actor: AccountInvitationListActor): Promise<AccountInvitationListItem[] | null> {
      const scope = accountInvitationListScope(actor);
      return scope ? repository.list(scope) : null;
    },
  };
}
