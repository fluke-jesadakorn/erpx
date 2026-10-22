export type LegalSigningAuthority = { id: string; version: number; document_id: string; checksum: string; approved_by: string };
export function createSigningAuthorityQuery(ports: { current(itemId: string, userId: string): Promise<LegalSigningAuthority | null>; error(status: number, code: string, message: string): Error }) {
  return async (itemId: string, userId: string) => {
    const row = await ports.current(itemId, userId);
    if (!row) throw ports.error(409, "LEGAL_SIGNING_AUTHORITY", "Record and independently approve current signing authority for every signer before sending or signing.");
    return row;
  };
}
