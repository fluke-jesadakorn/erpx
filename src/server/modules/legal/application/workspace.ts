export type LegalWorkspaceItem = { id: string; kind: string; title: string; state: string; due_date: string | null; owner_id: string; version: number };
export function createLegalWorkspaceQuery(ports: { require(): void; deadlines(): Promise<LegalWorkspaceItem[]>; assigned(): Promise<LegalWorkspaceItem[]> }) {
  return async () => { ports.require(); const [deadlines, assigned] = await Promise.all([ports.deadlines(), ports.assigned()]); return { deadlines, assigned }; };
}
