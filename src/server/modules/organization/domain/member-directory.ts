export type DirectoryAccount = { id: string; name: string; email: string };
export type DirectoryMember = DirectoryAccount & { personId: string | null };
export type MemberDirectoryActor = { userId: string; companyId: string; branchId: string | null; manager: boolean; beta: boolean };

export function projectDirectoryMembers(actor: MemberDirectoryActor, accounts: DirectoryAccount[], links: { accountId: string; personId: string | null }[]): DirectoryMember[] {
  const personIds = new Map(links.map(link => [link.accountId, link.personId]));
  return accounts.map(account => ({ ...account, email: actor.manager || account.id === actor.userId ? account.email : "", personId: personIds.get(account.id) ?? null }));
}

export function memberDirectoryBatches<T>(items: T[]): T[][] {
  const batches: T[][] = [];
  for (let offset = 0; offset < items.length; offset += 80) batches.push(items.slice(offset, offset + 80));
  return batches;
}
