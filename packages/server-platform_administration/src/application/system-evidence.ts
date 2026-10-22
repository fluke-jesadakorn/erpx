import type { Clock } from "@erpx/server-shared/application/ports";
import type { AttachmentManifestItem } from "@erpx/server-files/public/attachments/types";
import { systemEvidenceInput, systemEvidenceOwners, type SystemEvidenceTarget, type SystemEvidenceInput } from "@erpx/server-platform_administration/domain/system-evidence";
export type SystemEvidenceCommand = { id: string; input: SystemEvidenceInput; key: string; targetType: SystemEvidenceTarget };
export type SystemEvidenceTransaction = {
  lock(command: SystemEvidenceCommand): Promise<{ version: number; status: string } | null>;
  prepare(command: SystemEvidenceCommand, binding: { version: number; purpose: string; bindingId: string }): Promise<{ manifest: AttachmentManifestItem[]; commit(): Promise<void> }>;
  advance(command: SystemEvidenceCommand, now?: number): Promise<void>;
};
export function createSystemEvidenceApplication(ports: {
  authorizeCompanyWithMfa(): Promise<void>; clock: Clock;
  run<Result>(command: SystemEvidenceCommand, permission: string, action: (transaction: SystemEvidenceTransaction) => Promise<Result>): Promise<Result>;
  error(status: number, code: string, message: string): Error;
}) {
  return async (targetType: SystemEvidenceTarget, id: string, raw: unknown, key: string) => {
    const input = systemEvidenceInput.parse(raw), owner = systemEvidenceOwners[targetType], command = { targetType, id, input, key };
    await ports.authorizeCompanyWithMfa();
    return ports.run(command, owner.permission, async transaction => {
      const row = await transaction.lock(command);
      if (!row) throw ports.error(404, "SYSTEM_EVIDENCE_OWNER_NOT_FOUND", "The evidence owner is unavailable in this Company.");
      if (Number(row.version) !== input.expectedVersion) throw ports.error(409, "STALE_VERSION", "Reload the owner before binding its sanitized evidence.");
      const version = input.expectedVersion + 1, prepared = await transaction.prepare(command, { version, purpose: owner.purpose, bindingId: `${id}:evidence:${version}` });
      await prepared.commit();
      await transaction.advance(command, targetType === "people_provision_operation" ? ports.clock.now() : undefined);
      return { id, version, status: row.status, manifest: prepared.manifest };
    });
  };
}
