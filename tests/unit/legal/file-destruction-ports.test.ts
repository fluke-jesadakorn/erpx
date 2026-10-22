import { beforeEach, expect, it, vi } from 'vitest';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
const guards = vi.hoisted(() => ({ local: vi.fn(), remote: vi.fn() }));
vi.mock('@erpx/server-legal/adapters/runtime-preservation-authority', () => ({ runtimePreservationAuthority: () => ({ assertDestruction: guards.local }) }));
vi.mock('@erpx/server-legal/adapters/runtime-minio-preservation-authority', () => ({ runtimeMinioPreservationAuthority: () => ({ destruction: guards.remote }) }));
import { fileDestructionAuthority } from '@erpx/server-legal/adapters/file-destruction-ports';
beforeEach(() => { guards.local.mockReset(); guards.remote.mockReset(); });

it('refuses a deletion port rebound to another database or destruction before consulting owner policy', async () => {
  const database = {} as SqlDatabase, foreign = {} as SqlDatabase;
  const port = fileDestructionAuthority(database, 'destruction');
  await expect(port.assertLocal(foreign, 'destruction', ['legal/key'])).rejects.toThrow('LEGAL_DESTRUCTION_NOT_AUTHORIZED');
  await expect(port.assertLocal(database, 'other-destruction', ['legal/key'])).rejects.toThrow('LEGAL_DESTRUCTION_NOT_AUTHORIZED');
  await expect(port.readRemote(foreign, 'destruction')).rejects.toThrow('LEGAL_DESTRUCTION_NOT_AUTHORIZED');
  await expect(port.readRemote(database, 'other-destruction')).rejects.toThrow('LEGAL_DESTRUCTION_NOT_AUTHORIZED');
  expect(guards.local).not.toHaveBeenCalled(); expect(guards.remote).not.toHaveBeenCalled();
});
it('rechecks local owner authority after a previously allowed deletion without caching the permission', async () => {
  const database = {} as SqlDatabase, port = fileDestructionAuthority(database, 'destruction');
  guards.local.mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error('LEGAL_HOLD'));
  await expect(port.assertLocal(database, 'destruction', ['legal/key'])).resolves.toBeUndefined();
  await expect(port.assertLocal(database, 'destruction', ['legal/key'])).rejects.toThrow('LEGAL_HOLD');
  expect(guards.local).toHaveBeenCalledTimes(2);
  expect(guards.local).toHaveBeenNthCalledWith(2, ['legal/key'], 'destruction');
  expect(Object.isFrozen(port)).toBe(true);
});
it('reads remote destruction authority afresh and exposes its revocation to storage', async () => {
  const database = {} as SqlDatabase, port = fileDestructionAuthority(database, 'destruction');
  const row = { manifest: JSON.stringify({ keys: ['legal/key'] }) };
  guards.remote.mockResolvedValueOnce(row).mockResolvedValueOnce(null);
  await expect(port.readRemote(database, 'destruction')).resolves.toBe(row);
  await expect(port.readRemote(database, 'destruction')).resolves.toBeNull();
  expect(guards.remote).toHaveBeenCalledTimes(2);
  expect(guards.remote).toHaveBeenNthCalledWith(2, 'destruction');
});
