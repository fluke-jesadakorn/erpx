import 'server-only';
import type {SqlDatabase} from '../public/runtime/types';
export function diagnosedHoldingBinding(db: SqlDatabase, slug: string) { return db.prepare('SELECT holding_id,status FROM holding_bindings WHERE slug=?').bind(slug).first<{holding_id: string; status: string}>(); }
export function onboardingSlugBinding(db: SqlDatabase, slug: string) { return db.prepare('SELECT 1 FROM holding_bindings WHERE slug=?').bind(slug).first(); }
