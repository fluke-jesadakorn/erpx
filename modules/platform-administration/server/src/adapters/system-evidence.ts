import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { iamActorFromWork, assertCompanyScope } from "@erpx/identity-access-server/public/iam-context";
import { iamCommand } from "@erpx/identity-access-server/public/iam-transactions";
import { requireFreshMfa } from "@erpx/identity-access-server/public/iam-mfa";
import { lockSystemEvidenceTarget, advanceSystemEvidenceTarget } from "@erpx/identity-access-server/public/system-evidence-targets";
import { transactionDatabase, type PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { prepareAttachmentBinding } from "@erpx/files-server/public/attachments/service";
import { createSystemEvidenceApplication } from "@erpx/platform-administration-server/application/system-evidence";
import type { SystemEvidenceTarget } from "@erpx/platform-administration-server/domain/system-evidence";

function bindSystemEvidence(db: PostgresDatabase, work: WorkContext, targetType: SystemEvidenceTarget, id: string, raw: unknown, key: string) {
  // Actor conversion is deferred until the application has parsed the evidence.
  let actor: ReturnType<typeof iamActorFromWork>;
  return createSystemEvidenceApplication({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message), authorizeCompanyWithMfa: async () => { actor = iamActorFromWork(work); assertCompanyScope(actor); await requireFreshMfa(actor); },
    run: (command, permission, action) => iamCommand(db, actor, key, `${targetType}.evidence.bind`, { id, ...command.input }, permission, async client => {
      const tx = transactionDatabase(client);
      return action({ lock: () => lockSystemEvidenceTarget(client, targetType, id, actor.companyId), prepare: async (_command, binding) => {
        const prepared = await prepareAttachmentBinding(tx, work, { attachmentSetIds: command.input.attachmentSetIds, bindingContract: { targetType, bindingType: `${targetType}_evidence`, contexts: [{ purposeKey: binding.purpose, componentKind: "action", componentRef: `${targetType}:evidence` }] }, targetId: id, expectedTargetVersion: command.input.expectedVersion, commandId: key, bindingId: binding.bindingId });
        return { manifest: prepared.manifest, commit: async () => { await tx.batch([...prepared.guards, ...prepared.steps]); } };
      }, advance: (_command, now) => advanceSystemEvidenceTarget(client, targetType, id, actor.companyId, now) });
    }),
  })(targetType, id, raw, key);
}
export const bindIamReviewEvidence = (db: PostgresDatabase, actor: WorkContext, id: string, raw: unknown, key: string) => bindSystemEvidence(db, actor, "iam_review", id, raw, key);
export const bindIamMigrationEvidence = (db: PostgresDatabase, actor: WorkContext, id: string, raw: unknown, key: string) => bindSystemEvidence(db, actor, "iam_migration", id, raw, key);
export const bindProvisioningEvidence = (db: PostgresDatabase, actor: WorkContext, id: string, raw: unknown, key: string) => bindSystemEvidence(db, actor, "people_provision_operation", id, raw, key);
