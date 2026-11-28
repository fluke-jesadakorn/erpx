import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
export const posPermissions = (actor: WorkContext) => Object.fromEntries(['pos.read', 'pos.sell', 'pos.kitchen', 'pos.approve', 'pos.configure', 'pos.accounting.prepare', 'pos.accounting.approve', 'pos.payments.configure', 'pos.reports', 'finance.read', 'finance.write', 'finance.approve', 'pms.approve'].map(p => [p, hasPermission(actor, p)]));
