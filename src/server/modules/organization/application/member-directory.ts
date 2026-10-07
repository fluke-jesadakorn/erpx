import type { Clock } from "../../shared";
import { memberDirectoryBatches, projectDirectoryMembers, type DirectoryAccount, type DirectoryMember, type MemberDirectoryActor } from "../domain/member-directory";

export interface MemberDirectoryRepository {
  memberIds(actor: MemberDirectoryActor, now: number): Promise<string[]>;
  accounts(ids: string[]): Promise<DirectoryAccount[]>;
  personLinks(companyId: string, accounts: DirectoryAccount[], allowBetaEmailFallback: boolean): Promise<{ accountId: string; personId: string | null }[]>;
}

export function createMemberDirectoryApplication(actor: MemberDirectoryActor, repository: MemberDirectoryRepository, clock: Clock) {
  return {
    async read(): Promise<DirectoryMember[]> {
      const members = await repository.memberIds(actor, clock.now());
      const rows: DirectoryMember[] = [];
      for (const ids of memberDirectoryBatches(members)) {
        const accounts = await repository.accounts(ids);
        const links = accounts.length ? await repository.personLinks(actor.companyId, accounts, actor.beta && actor.manager) : [];
        rows.push(...projectDirectoryMembers(actor, accounts, links));
      }
      return rows.sort((a, b) => a.name.localeCompare(b.name));
    },
  };
}
