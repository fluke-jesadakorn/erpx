import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { SqlValueRow } from '@erpx/server-runtime/public/runtime/sql';
import { nativePosAll } from './native-owner-write';

/**
 * Owner-issued POS add-on/package obligation reads for caller-owned
 * transactions. The SQL lives in the native POS owner registry
 * (`erpx-server-pos::adapters::obligation_reads`); this bridge sends a typed
 * kind and positional Company/package values only. The caller executes the
 * statement on its own serializable unit, so no TypeScript SQL, mirror or
 * fallback reader exists for the facts reads. The settlement-source
 * projections consumed by Finance/Inventory/PMS are SQL text, not row reads,
 * and stay at the cross-owner adapter boundary.
 */

const ADDON_PACKAGES = ['pos-restaurant', 'pos-hotel-charge'] as const;
type AddonPackage = (typeof ADDON_PACKAGES)[number];

function addonKind(packageId: string): string {
  if (!ADDON_PACKAGES.includes(packageId as AddonPackage))
    throw new Error('POS_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED');
  return packageId === 'pos-restaurant'
    ? 'pos_addon_obligations_restaurant'
    : 'pos_addon_obligations_hotel';
}

/** Owner-issued restaurant/hotel addon obligation facts. */
export async function nativePosAddonObligationsStatement(
  db: SqlDatabase,
  company: string,
  packageId: string,
): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, addonKind(packageId), [company, packageId]);
}

/** Owner-issued core POS package obligation facts. */
export async function nativePosPackageObligationsStatement(
  db: SqlDatabase,
  company: string,
): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, 'pos_package_obligations', [company]);
}

/** Owner-issued PMS package obligation facts (Hotel v1). */
export async function nativePosPmsPackageObligationsStatement(
  db: SqlDatabase,
  company: string,
): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, 'pms_pos_package_obligations', [company]);
}
