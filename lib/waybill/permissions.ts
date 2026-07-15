// lib/waybill/permissions.ts
//
// Stage-level permission checks for Waybill operations.
// `canActorAttachAt(actorRole, stage)` is the gate used by the upload
// form and the `attachWaybillDocumentAction`. CFO/CEO/admin always-allow.
// Terminal stages (disbursed/rejected) are read-only.

import { normalizeStage, stageRoles } from '../perm/stages';

export type ActorRole = string;

export interface ActorContext {
  id: number;
  roleName: ActorRole;
  isAdmin?: boolean;
}

const ALWAYS_ALLOW: ReadonlySet<ActorRole> = new Set<ActorRole>([
  'cfo',
  'ceo',
  'admin',
]);

export function canActorAttachAt(actorRole: ActorRole, stage: string): boolean {
  if (ALWAYS_ALLOW.has(actorRole)) return stage !== 'disbursed';
  const canon = normalizeStage(stage);
  if (!canon) return false;
  if (canon === 'disbursed' || canon === 'rejected') return false;
  const required = stageRoles(canon);
  if (required.length === 0) return false;
  return required.includes(actorRole);
}

export function canActorRemoveAttachment(actor: ActorContext): boolean {
  return actor.isAdmin === true || ALWAYS_ALLOW.has(actor.roleName);
}

export function isTerminalStage(stage: string): boolean {
  const canon = normalizeStage(stage);
  return canon === 'disbursed' || canon === 'rejected';
}

