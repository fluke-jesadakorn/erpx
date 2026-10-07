import "server-only";
import type { WorkContext } from "../../identity_access/public/authorization";
import { iamActorFromWork, assertCompanyScope } from "../../identity_access/public/iam-context";
import { iamCommand } from "../../identity_access/public/iam-transactions";
import { requireFreshMfa } from "../../identity_access/public/iam-mfa";
import { lockSystemEvidenceTarget, advanceSystemEvidenceTarget } from "../../identity_access/public/system-evidence-targets";
import { transactionDatabase, type PostgresDatabase } from "../../runtime/public/runtime/postgres";
import { ApiError } from "../../runtime/public/http";
import { prepareAttachmentBinding } from "../../files/public/attachments/service";
import { createSystemEvidenceApplication } from "../application/system-evidence";
import type { SystemEvidenceTarget } from "../domain/system-evidence";

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
