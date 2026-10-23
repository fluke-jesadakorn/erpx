import { appDefinitions, appDirectory } from '@erpx/contracts/apps';

export type ProcessIdentity = { command: string; cwd: string; started: string };
export type AppProcessLease = { id: string; pid: number; port: number; identity: ProcessIdentity };
export type GatewayProcessLease = { version: 2; root: string; gatewayPid: number; apps: AppProcessLease[] };
export type RecoveryPorts = {
  identity(pid: number): ProcessIdentity | null;
  stop(pid: number): void;
  waitForExit(pid: number): Promise<void>;
};

/** PID reuse never confers ownership. Check the entire lease before signaling. */
export async function recoverGatewayApps(input: unknown, root: string, ports: RecoveryPorts, assignedPorts?: ReadonlyMap<string,number>) {
  const lease = input as GatewayProcessLease;
  if (!lease || lease.root !== root || !Array.isArray(lease.apps) || !Number.isSafeInteger(lease.gatewayPid))
    throw new Error('GATEWAY_LEASE_IDENTITY_REQUIRED');
  if (ports.identity(lease.gatewayPid)) throw new Error('GATEWAY_LEASE_OWNER_STILL_ALIVE');
  if (!lease.apps.length) return;
  if (lease.version !== 2) throw new Error('OLD_GATEWAY_LEASE_REQUIRES_MANUAL_REVIEW');
  const owned: AppProcessLease[] = [];
  const seen = new Set<number>();
  for (const app of lease.apps) {
    const port = assignedPorts ? assignedPorts.get(app.id) : app.id === 'compat' ? 3200 : appDefinitions.find(entry => entry.id === app.id)?.port;
    if (!port || port < 3200 || port > 3299 || app.port !== port || !Number.isSafeInteger(app.pid) || app.pid <= 1 || seen.has(app.pid))
      throw new Error('INVALID_APP_PROCESS_LEASE');
    seen.add(app.pid);
    const identity = ports.identity(app.pid);
    if (!identity) continue;
    if (!app.identity || identity.cwd !== root || identity.cwd !== app.identity.cwd || identity.started !== app.identity.started ||
      identity.command !== app.identity.command || !identity.command.includes('/next/dist/bin/next dev ') ||
      !identity.command.includes(`${root}/${appDirectory(app.id)} `) || !identity.command.includes(`--port ${port} `))
      throw new Error('APP_PROCESS_OWNERSHIP_CHANGED');
    owned.push(app);
  }
  // Validate every process first: a foreign entry prevents partial cleanup.
  for (const app of owned) {
    const current = ports.identity(app.pid);
    if (!current) continue;
    if (current.command !== app.identity.command || current.cwd !== app.identity.cwd || current.started !== app.identity.started) throw new Error('APP_PROCESS_OWNERSHIP_CHANGED');
    ports.stop(app.pid);
    await ports.waitForExit(app.pid);
  }
}
