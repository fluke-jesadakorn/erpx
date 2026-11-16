import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function diagnosedHoldingBinding(db: SqlDatabase, slug: string) { return db.prepare("SELECT 1").bind().first<{holding_id: string; status: string}>(); }
export function onboardingSlugBinding(db: SqlDatabase, slug: string) { return db.prepare("SELECT 1").bind().first(); }
