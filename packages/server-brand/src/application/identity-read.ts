import { brandIdentitySchema, type ActiveBrandIdentity, type BrandCategoryLock, type BrandDraft, type BrandPublished } from "@erpx/server-brand/domain/identity";

export type BrandIdentityEvent = { id: string; action: string; record_id: string; version: number; reason: string; actor_id: string; created_at: number; details_json: string };
export interface PublishedBrandIdentityPorts {
  available(): Promise<boolean>;
  active(): Promise<(BrandPublished & { active_version: number }) | null>;
}
export async function readPublishedBrandIdentity(ports: PublishedBrandIdentityPorts): Promise<ActiveBrandIdentity | null> {
  if (!await ports.available()) return null;
  const row = await ports.active();
  return row ? { ...row, identity: brandIdentitySchema.parse(JSON.parse(row.identity_json)) } : null;
}
export interface BrandIdentityWorkspacePorts {
  actorId: string;
  canManage: boolean;
  canLock: boolean;
  authorize(): Promise<void>;
  drafts(includeOthers: boolean): Promise<BrandDraft[]>;
  versions(): Promise<BrandPublished[]>;
  locks(): Promise<BrandCategoryLock[]>;
  history(): Promise<BrandIdentityEvent[]>;
  active(): Promise<ActiveBrandIdentity | null>;
  legacy(): Promise<{ shortName: string; accentColor: string }>;
  media(): Promise<{ id: string; name: string }[]>;
}
export async function readBrandIdentityWorkspace(ports: BrandIdentityWorkspacePorts) {
  await ports.authorize();
  const [drafts, versions, locks, history, active, legacy, media] = await Promise.all([
    ports.drafts(ports.canManage), ports.versions(), ports.locks(), ports.history(), ports.active(), ports.legacy(), ports.media(),
  ]);
  const initial = active?.identity ?? { identity: { shortName: legacy.shortName, tagline: "" }, visual: { accentColor: legacy.accentColor, logoPublicationId: null } };
  return { drafts, versions, locks, history: ports.canManage ? history : [], active, initial, media, actorId: ports.actorId, canManage: ports.canManage, canLock: ports.canLock };
}
