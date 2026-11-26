import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {Row} from '@erpx/server-commerce/public/commerce';
import { nativeSupportRuntime } from "@erpx/server-support/adapters/command-runtime";
import { nativeSupportReadAll, SUPPORT_RD_SLA_POLICY_POINT, SUPPORT_RD_SLA_POLICY_WORK } from "@erpx/server-support/adapters/native-support-read";
import { createGetSupportPolicy, createSupportPolicyWork } from "@erpx/server-support/application/support-sla";
export { supportPolicyCreateSchema, supportPolicyChangeSchema, supportClockAttachSchema, supportClockValue } from "@erpx/server-support/domain/support-sla";
export async function getSupportPolicy(a: WorkContext, id: string) { const runtime = nativeSupportRuntime(a); await runtime.requirePurchase("support.sla"); return createGetSupportPolicy({ ...runtime, policy: async (a, id) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_SLA_POLICY_POINT, [id, a.companyId], write)).results[0] ?? null; } })(a, id); }
export async function supportPolicyWork(a: WorkContext) { const runtime = nativeSupportRuntime(a), access = await runtime.purchase("support.sla"); if (!access.history) return []; const items = await createSupportPolicyWork({ policies: async (a) => { const write = await runtime.database(); return (await nativeSupportReadAll<Row>(SUPPORT_RD_SLA_POLICY_WORK, [a.userId, a.companyId], write)).results; } })(a); return items.map(item => ({ ...item, canAct: item.canAct && access.newWork, queue: item.queue === "todo" && !access.newWork ? "waiting" as const : item.queue })); }
