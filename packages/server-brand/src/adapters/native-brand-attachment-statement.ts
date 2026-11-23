import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeNotificationStatement } from '@erpx/server-collaboration/adapters/native-notification-statement';

type BrandTarget = 'holding_brand' | 'branch_brand' | 'company_storefront_brand';

/** Native-owned Brand attachment-source image statements. The SQL text and
 * Company bind are issued by the native owner against the authorized
 * session; the caller only supplies the target kind/id and executes the
 * issued statement inside its own serializable unit. */
async function brandStatement(
  db: SqlDatabase,
  actor: WorkContext,
  kind: 'brand_attachment_image' | 'brand_organization_image',
  targetType: BrandTarget,
  targetId: string,
  key: string,
  now: number,
): Promise<string | null> {
  const statement = await nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: actor.userId,
      recordType: 'brand_attachment',
      recordId: targetId,
      recordVersion: 1,
      message: 'brand attachment read',
      kind,
      targetKind: targetType,
      targetId,
    },
    key,
    now,
  );
  return statement.first<string>('image');
}

export function nativeBrandAttachmentImage(
  db: SqlDatabase,
  actor: WorkContext,
  targetType: BrandTarget,
  targetId: string,
  key: string,
  now: number,
): Promise<string | null> {
  return brandStatement(db, actor, 'brand_attachment_image', targetType, targetId, key, now);
}

export function nativeBrandOrganizationImage(
  db: SqlDatabase,
  actor: WorkContext,
  targetType: BrandTarget,
  targetId: string,
  key: string,
  now: number,
): Promise<string | null> {
  return brandStatement(db, actor, 'brand_organization_image', targetType, targetId, key, now);
}
