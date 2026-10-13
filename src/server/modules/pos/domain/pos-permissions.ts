import type { WorkContext } from "../../identity_access/public/authorization";
import { hasPermission } from "@/lib/identity_access/permissions";
export const posPermissions = (actor: WorkContext) => Object.fromEntries(['pos.read', 'pos.sell', 'pos.kitchen', 'pos.approve', 'pos.configure', 'pos.accounting.prepare', 'pos.accounting.approve', 'pos.payments.configure', 'pos.reports', 'finance.read', 'finance.write', 'finance.approve', 'pms.approve'].map(p => [p, hasPermission(actor, p)]));
