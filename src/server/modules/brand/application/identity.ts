import type { Clock } from "../../shared";
import { brandIdentitySchema, brandDraftInput, brandPublishInput, brandRollbackInput, brandCategoryLockInput, brandCancelInput, type ActiveBrandIdentity, type BrandCategoryLock, type BrandDraft, type BrandIdentity, type BrandPublished } from "../domain/identity";
import { assertBrandCategoryLocks, assertBrandIndependentPublisher, BrandRuleError } from "../domain/policy";

type MutationCommon = { key: string; operation: string; recordId: string; version: number; input: unknown; now: number };
export type BrandMutation = MutationCommon & (
  | { action: "save"; identity: BrandIdentity; locks: BrandCategoryLock[]; existing: BrandDraft | null; active: ActiveBrandIdentity | null; reason: string; expectedPublishedVersion: number; expectedVersion?: number }
  | { action: "publish"; identity: BrandIdentity; locks: BrandCategoryLock[]; draft: BrandDraft; publishedId: string; revision: number; expectedPublishedVersion: number; expectedVersion: number; reason: string }
  | { action: "rollback"; identity: BrandIdentity; locks: BrandCategoryLock[]; published: BrandPublished; expectedPublishedVersion: number; reason: string }
  | { action: "lock"; category: "identity" | "visual"; locked: boolean; active: ActiveBrandIdentity; expectedVersion: number; expectedPublishedVersion: number; reason: string }
  | { action: "cancel"; expectedVersion: number; reason: string }
);

export interface BrandIdentityPorts {
  companyId: string;
  clock: Clock;
  isSamePerson(userId: string): boolean;
  authorize(write: boolean, owner?: boolean): Promise<void>;
  replay(key: string, operation: string, recordId: string, input: unknown): Promise<Record<string, unknown> | null>;
  active(): Promise<ActiveBrandIdentity | null>;
  locks(): Promise<BrandCategoryLock[]>;
  draft(id: string): Promise<BrandDraft | null>;
  published(id: string): Promise<BrandPublished | null>;
  nextRevision(): Promise<number>;
  commit<Result>(mutation: BrandMutation, result: Result): Promise<Result>;
}

/** All state decisions use snapshots; the adapter repeats their guards at commit. */
export function createBrandIdentityApplication(ports: BrandIdentityPorts) {
  return {
    async save(raw: unknown, key: string) {
      const input = brandDraftInput.parse(raw);
      await ports.authorize(true);
      const recordId = input.id ?? `brand-draft-${key}`;
      const operation = input.id ? "brand.identity.save" : "brand.identity.draft";
      const prior = await ports.replay(key, operation, recordId, input);
      if (prior) return prior;
      const active = await ports.active();
      const locks = await ports.locks();
      assertBrandCategoryLocks(locks, input.identity);
      if (input.expectedPublishedVersion !== (active?.active_version ?? 0)) throw new BrandRuleError(409, "BRAND_PUBLICATION_CHANGED", "The published identity changed. Review it before saving.");
      const existing = input.id ? await ports.draft(input.id) : null;
      if (input.id && (!existing || existing.status !== "draft")) throw new BrandRuleError(404, "BRAND_DRAFT_UNAVAILABLE", "The editable draft is unavailable.");
      if (existing && !input.expectedVersion) throw new BrandRuleError(422, "EXPECTED_VERSION_REQUIRED", "Use the current draft version.");
      const version = (input.expectedVersion ?? 0) + 1;
      return ports.commit({ action: "save", key, operation, recordId, version, input, now: ports.clock.now(), identity: input.identity, active, locks, existing, reason: input.reason, expectedPublishedVersion: input.expectedPublishedVersion, expectedVersion: input.expectedVersion }, { id: recordId, status: "draft", version });
    },
    async publish(draftId: string, raw: unknown, key: string) {
      const input = brandPublishInput.parse(raw);
      await ports.authorize(true);
      const operation = "brand.identity.publish";
      const prior = await ports.replay(key, operation, draftId, input);
      if (prior) return prior;
      const draft = await ports.draft(draftId);
      if (!draft || draft.status !== "draft") throw new BrandRuleError(404, "BRAND_DRAFT_UNAVAILABLE", "The editable draft is unavailable.");
      assertBrandIndependentPublisher(ports.isSamePerson, draft.created_by, draft.updated_by);
      const identity = brandIdentitySchema.parse(JSON.parse(draft.identity_json));
      const active = await ports.active();
      const locks = await ports.locks();
      assertBrandCategoryLocks(locks, identity);
      if (draft.based_revision !== (active?.revision ?? 0)) throw new BrandRuleError(409, "BRAND_DRAFT_REBASE_REQUIRED", "The published identity advanced. Save a reviewed draft against the current identity first.");
      const now = ports.clock.now();
      const revision = await ports.nextRevision();
      const publishedId = `brand-version-${key}`;
      const version = input.expectedVersion + 1;
      return ports.commit({ action: "publish", key, operation, recordId: draftId, version, input, now, identity, locks, draft, publishedId, revision, expectedPublishedVersion: input.expectedPublishedVersion, expectedVersion: input.expectedVersion, reason: input.reason }, { id: draftId, status: "published", version, publishedId, revision, publishedVersion: input.expectedPublishedVersion + 1 });
    },
    async rollback(raw: unknown, key: string) {
      const input = brandRollbackInput.parse(raw);
      await ports.authorize(true);
      const operation = "brand.identity.rollback";
      const prior = await ports.replay(key, operation, ports.companyId, input);
      if (prior) return prior;
      const published = await ports.published(input.publishedId);
      if (!published) throw new BrandRuleError(404, "BRAND_VERSION_UNAVAILABLE", "Choose a published identity from this Company.");
      const identity = brandIdentitySchema.parse(JSON.parse(published.identity_json));
      const locks = await ports.locks();
      assertBrandCategoryLocks(locks, identity);
      const version = input.expectedPublishedVersion + 1;
      return ports.commit({ action: "rollback", key, operation, recordId: ports.companyId, version, input, now: ports.clock.now(), identity, locks, published, expectedPublishedVersion: input.expectedPublishedVersion, reason: input.reason }, { id: published.id, revision: published.revision, publishedVersion: version });
    },
    async changeLock(raw: unknown, key: string) {
      const input = brandCategoryLockInput.parse(raw);
      await ports.authorize(true, true);
      const recordId = `${ports.companyId}:${input.category}`;
      const operation = input.locked ? "brand.category.lock" : "brand.category.unlock";
      const prior = await ports.replay(key, operation, recordId, input);
      if (prior) return prior;
      const active = await ports.active();
      if (!active) throw new BrandRuleError(409, "BRAND_PUBLISHED_IDENTITY_REQUIRED", "Publish an independently reviewed identity before locking categories.");
      const version = input.expectedVersion + 1;
      return ports.commit({ action: "lock", key, operation, recordId, version, input, now: ports.clock.now(), category: input.category, locked: input.locked, active, expectedVersion: input.expectedVersion, expectedPublishedVersion: input.expectedPublishedVersion, reason: input.reason }, { category: input.category, locked: input.locked, version });
    },
    async cancel(draftId: string, raw: unknown, key: string) {
      const input = brandCancelInput.parse(raw);
      await ports.authorize(true);
      const operation = "brand.identity.cancel";
      const prior = await ports.replay(key, operation, draftId, input);
      if (prior) return prior;
      const version = input.expectedVersion + 1;
      return ports.commit({ action: "cancel", key, operation, recordId: draftId, version, input, now: ports.clock.now(), expectedVersion: input.expectedVersion, reason: input.reason }, { id: draftId, status: "cancelled", version });
    },
  };
}
