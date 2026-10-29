/** Display data and optimistic selection hints only; never authentication tokens. */
export type DeviceAccount = { handle: string; name: string; email: string; image: string | null; current: boolean };
export type DeviceAccounts = { initialized: boolean; revision: number; accounts: DeviceAccount[]; maximum: number };
export type SwitcherRole = { id: string; label: string; version: string | number; selected: boolean };
export type SwitcherWorkspace = { key: string; label: string; kind: 'work' | 'admin' | 'front'; productId: string; companyId: string; target: string };
export type SwitcherContext = {
  user: { id: string; name: string; email: string };
  workspaces: SwitcherWorkspace[];
  workspace: SwitcherWorkspace | null;
  roles: SwitcherRole[];
  revision: number;
  provider: { accountKey: string; contextKey: string; selectedId: string; revision: number; choices: { id: string; label: string; version: string }[] } | null;
};
