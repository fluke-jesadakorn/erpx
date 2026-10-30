import { appDefinitions, appDirectory } from '@erpx/contracts/apps';

export type ProcessIdentity = { command: string; cwd: string; started: string };
export type AppProcessLease = { id: string; pid: number; port: number; identity: ProcessIdentity };
export type PidNamespace = { bootId: string; namespace: string };
export type GatewayProcessLease = { version: 2 | 3; root: string; gatewayPid: number; apps: AppProcessLease[]; pidNamespace?: PidNamespace };
/** Only a confirmed absent PID converts an inspection error into absence. */
export function inspectProcessIdentity(pid: number, read: (pid: number) => ProcessIdentity, probe: (pid: number) => void): ProcessIdentity | null {
  try { return read(pid); } catch (error) {
    try { probe(pid); } catch (probeError) { if ((probeError as NodeJS.ErrnoException).code === 'ESRCH') return null; }
    throw error;
  }
}
const validNamespace = (value: PidNamespace | undefined): value is PidNamespace => !!value && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value.bootId) && /^pid:\[\d+\]$/.test(value.namespace);
export type RecoveryPorts = {
  identity(pid: number): ProcessIdentity | null;
  stop(pid: number): void;
  waitForExit(pid: number): Promise<void>;
};

/** PID reuse never confers ownership. Check the entire lease before signaling. */
export async function recoverGatewayApps(input: unknown, root: string, ports: RecoveryPorts, assignedPorts?: ReadonlyMap<string,number>, currentNamespace?: PidNamespace) {
  const lease = input as GatewayProcessLease;
  if (!lease || lease.root !== root || !Array.isArray(lease.apps) || !Number.isSafeInteger(lease.gatewayPid))
    throw new Error('GATEWAY_LEASE_IDENTITY_REQUIRED');
  if (lease.version !== 2 && lease.version !== 3) throw new Error('OLD_GATEWAY_LEASE_REQUIRES_MANUAL_REVIEW');
  if (lease.version === 3 && !validNamespace(lease.pidNamespace)) throw new Error('GATEWAY_PID_NAMESPACE_INVALID');
  const oldNamespace = lease.version === 3 && validNamespace(currentNamespace) && (lease.pidNamespace!.bootId !== currentNamespace.bootId || lease.pidNamespace!.namespace !== currentNamespace.namespace);
  if (!oldNamespace && ports.identity(lease.gatewayPid)) throw new Error('GATEWAY_LEASE_OWNER_STILL_ALIVE');
  const owned: AppProcessLease[] = [];
  const seen = new Set<number>();
  for (const app of lease.apps) {
    const port = assignedPorts ? assignedPorts.get(app.id) : app.id === 'compat' ? 3200 : appDefinitions.find(entry => entry.id === app.id)?.port;
    if (!port || port < 3200 || port > 3299 || app.port !== port || !Number.isSafeInteger(app.pid) || app.pid <= 1 || seen.has(app.pid))
      throw new Error('INVALID_APP_PROCESS_LEASE');
    seen.add(app.pid);
    if (oldNamespace) {
      if (!app.identity || app.identity.cwd !== root || typeof app.identity.started !== 'string' || !app.identity.started || typeof app.identity.command !== 'string' ||
        !app.identity.command.includes('/next/dist/bin/next dev ') || !app.identity.command.includes(`${root}/${appDirectory(app.id)} `) || !app.identity.command.includes(`--port ${port} `))
        throw new Error('APP_PROCESS_OWNERSHIP_CHANGED');
      continue;
    }
    const identity = ports.identity(app.pid);
    if (!identity) continue;
    if (!app.identity || identity.cwd !== root || identity.cwd !== app.identity.cwd || identity.started !== app.identity.started ||
      identity.command !== app.identity.command || !identity.command.includes('/next/dist/bin/next dev ') ||
      !identity.command.includes(`${root}/${appDirectory(app.id)} `) || !identity.command.includes(`--port ${port} `))
      throw new Error('APP_PROCESS_OWNERSHIP_CHANGED');
    owned.push(app);
  }
  if (oldNamespace) return true;
  // Validate every process first: a foreign entry prevents partial cleanup.
  for (const app of owned) {
    const current = ports.identity(app.pid);
    if (!current) continue;
    if (current.command !== app.identity.command || current.cwd !== app.identity.cwd || current.started !== app.identity.started) throw new Error('APP_PROCESS_OWNERSHIP_CHANGED');
    ports.stop(app.pid);
    await ports.waitForExit(app.pid);
  }
  return false;
}

/** Before replacing a lease from another namespace, occupied listeners remain foreign. */
export async function assertRetiredLeasePortsAvailable(lease: GatewayProcessLease, gatewayPort: number, occupied: (port: number) => Promise<boolean>) {
  if (await occupied(gatewayPort)) throw new Error(`GATEWAY_PORT_OCCUPIED:${gatewayPort}`);
  for (const app of lease.apps) if (await occupied(app.port)) throw new Error(`APP_PORT_OCCUPIED:${app.id}:${app.port}`);
}
