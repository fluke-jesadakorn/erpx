import type { generateProviderJson } from '../../agent/public/ai-transport';
import { type AssistantSource } from "@/lib/ai/workspace-assistant";
import { guidedIntakeModelSchema, intakeFields, type GuidedIntakeResult, type IntakeIntent, type guidedIntakeInputSchema } from "@/lib/ai/guided-intake-domain";
import type { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
export type ModelOutput = z.infer<typeof guidedIntakeModelSchema>;
export type Input = z.infer<typeof guidedIntakeInputSchema>;
export type Generate = typeof generateProviderJson;
export type Ref = GuidedIntakeResult["sources"][number];
export const scopeKey = (actor: WorkContext) => JSON.stringify([actor.holdingId, actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId, actor.iam?.roleRevision, actor.iam?.identityVersion]);
export const sourceRef = ({ id, title, path, version }: AssistantSource): Ref => ({ id, title, path, version });
export const labels: Record<Exclude<IntakeIntent, "unknown">, string> = {
    purchase: "Buy something", expense: "Claim an expense", leave: "Request leave",
    task: "Create a task", post: "Post an update",
};
export function explicitPurchaseQuantity(message: string, itemName: string) {
    const item = itemName.trim();
    if (!item || !message.toLocaleLowerCase().includes(item.toLocaleLowerCase()))
        return null;
    const escaped = item.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const english: Record<string, string> = { one: "1", two: "2", three: "3", four: "4", five: "5", six: "6", seven: "7", eight: "8", nine: "9", ten: "10" };
    const thai: Record<string, string> = { หนึ่ง: "1", สอง: "2", สาม: "3", สี่: "4", ห้า: "5", หก: "6", เจ็ด: "7", แปด: "8", เก้า: "9", สิบ: "10" };
    const candidates = new Set<string>();
    const before = new RegExp(`(?:^|[^\\p{L}\\d])([0-9]+(?:\\.[0-9]+)?|${Object.keys(english).join("|")})\\s+${escaped}(?:s)?(?=$|[^\\p{L}])`, "giu");
    for (const match of message.matchAll(before))
        candidates.add(english[match[1].toLowerCase()] ?? match[1]);
    for (const [word, count] of Object.entries(thai)) {
        if (message.includes(`${word}${item}`) || message.includes(`${item}${word}`))
            candidates.add(count);
    }
    const labelled = message.match(/\bquantity\s*:?\s*(\d+(?:\.\d+)?)/i);
    if (labelled)
        candidates.add(labelled[1]);
    return candidates.size === 1 && Number([...candidates][0]) > 0 ? [...candidates][0] : null;
}
export function cleanDraft(intent: IntakeIntent, raw: Record<string, string>, message: string) {
    if (intent === "unknown")
        return {};
    const result: Record<string, string> = {};
    for (const key of intakeFields[intent]) {
        const value = raw[key]?.trim() ?? "";
        if (!value)
            continue;
        if (["startDate", "endDate", "expenseDate", "neededDate", "dueDate"].includes(key)) {
            const day = value.slice(0, 10), parsed = new Date(`${day}T00:00:00.000Z`);
            if (!/^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2})?$/.test(value) || !message.includes(value) ||
                Number.isNaN(parsed.valueOf()) || parsed.toISOString().slice(0, 10) !== day)
                continue;
        }
        if (["amount", "quantity"].includes(key) &&
            (!/^\d+(?:\.\d{1,2})?$/.test(value) || Number(value) <= 0 || !new RegExp(`(^|[^\\d.])${value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![\\d.])`).test(message)))
            continue;
        if (key === "paymentIntent" && !["reimbursement", "company_payment"].includes(value))
            continue;
        if (key === "currency" && (!["THB", "USD", "EUR", "GBP", "JPY", "SGD"].includes(value.toUpperCase()) ||
            !(message.toUpperCase().includes(value.toUpperCase()) || (value.toUpperCase() === "THB" && /บาท|baht/i.test(message)))))
            continue;
        if (key === "purpose" && !["update", "plan", "blocker", "handover", "announcement"].includes(value))
            continue;
        result[key] = (key === "currency" ? value.toUpperCase() : value).slice(0, key === "body" || key === "description" ? 3000 : 300);
    }
    if (intent === "purchase") {
        const item = result.itemName ?? "";
        const quantity = explicitPurchaseQuantity(message, item);
        if (quantity)
            result.quantity = quantity;
        else
            delete result.quantity;
        if (!result.title && item && message.toLocaleLowerCase().includes(item.toLocaleLowerCase()))
            result.title = item;
    }
    return result;
}
