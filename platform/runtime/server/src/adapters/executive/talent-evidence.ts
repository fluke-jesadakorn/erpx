import { recordExecutiveEvidenceAccess } from '@erpx/platform-runtime-ui/adapters/server/native-executive-evidence';
import "server-only";
import { nativeHoldingConfiguration } from "@erpx/platform-native-sdk/holding-target";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { executiveEvidenceSources } from "@erpx/talent-server/public/executive-evidence-sources";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { currentAuthority } from "@erpx/platform-runtime-server/public/executive/access";
import { metricCatalogFor } from "@erpx/platform-runtime-server/public/executive/metrics";
import { createTalentEvidenceApplication, type TalentEvidenceRepository } from '@erpx/platform-runtime-server/application/executive/talent-evidence';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import type { TalentEvidenceRow } from "@erpx/platform-runtime-server/domain/executive/talent-evidence";

const ENV = "ERPX_NATIVE_TALENT_EVIDENCE_CONFIG";
const fail = (code = "NATIVE_TALENT_EVIDENCE_UNAVAILABLE", status = 503): never => {
    throw new ApiError(status, code, "The native Talent evidence service is unavailable.");
};
const record = (value: unknown): value is Record<string, unknown> =>
    value !== null && typeof value === "object" && !Array.isArray(value);
const boundedText = (value: unknown, max: number): value is string =>
    typeof value === "string" && new TextEncoder().encode(value).length > 0
    && new TextEncoder().encode(value).length <= max;
const validSlug = (value: unknown): value is string =>
    boundedText(value, 128) && /^[a-z0-9_-]+$/.test(value as string);
const validId = (value: unknown): value is string =>
    boundedText(value, 200) && /^[A-Za-z0-9._:-]+$/.test(value as string);

type Config = ReturnType<typeof nativeHoldingConfiguration>;

/** Strict native transport for the mounted Talent evidence point read. The
 * axum owner executes the SQL and returns the complete owner row or `null`;
 * this adapter emits no SQL, accepts no SQL and keeps no TypeScript fallback.
 * Missing, malformed or failing native service fails closed with ApiError(503). */
export async function nativeTalentEvidenceRow(
    actor: WorkContext,
    metric: "open-requisitions" | "application-stage-counts",
    recordId: string,
    range?: { from: number; toExclusive: number },
): Promise<TalentEvidenceRow | null> {
    const raw = process.env[ENV];
    if (!raw) return fail("NATIVE_TALENT_EVIDENCE_CONFIGURATION_REQUIRED");
    let config: Config;
    try { config = nativeHoldingConfiguration(raw); }
    catch { return fail("NATIVE_TALENT_EVIDENCE_CONFIGURATION_INVALID"); }

    if (!validId(actor.holdingId) || !validId(actor.companyId) || !validSlug(actor.holdingSlug)
        || !validId(recordId))
        return fail("NATIVE_TALENT_EVIDENCE_REQUEST_INVALID", 422);
    const target = config.holdings.find(item =>
        item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug);
    if (!target || !actor.authority || !actor.iam)
        return fail("WORK_CONTEXT_CHANGED", 409);

    const headersToSend = new Headers({
        accept: "application/json",
        "cache-control": "no-cache",
        "x-erpx-context-company": actor.companyId,
        "x-erpx-context-actor": actor.userId,
    });
    if (actor.branchId) headersToSend.set("x-erpx-context-branch", actor.branchId);
    if (actor.departmentId) headersToSend.set("x-erpx-context-department", actor.departmentId);
    if (range) {
        headersToSend.set("x-erpx-evidence-from", String(range.from));
        headersToSend.set("x-erpx-evidence-to", String(range.toExclusive));
    }

    const url = new URL(
        `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/talent/executive-evidence/${encodeURIComponent(metric)}/${encodeURIComponent(recordId)}`,
        config.privateBaseUrl,
    );
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
        const response = await fetch(url, {
            method: "GET",
            headers: headersToSend,
            cache: "no-store",
            redirect: "error",
            signal: controller.signal,
        });
        if (response.status !== 200) {
            const error = responseCode(await response.json().catch((): unknown => null));
            void response.body?.cancel().catch(() => undefined);
            return fail(error ?? "NATIVE_TALENT_EVIDENCE_UNAVAILABLE",
                error && response.status >= 400 && response.status <= 599 ? response.status : 503);
        }
        if (response.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json" || !response.body)
            return fail("NATIVE_TALENT_EVIDENCE_RESPONSE_INVALID", 502);
        const reader = response.body.getReader();
        const chunks: Uint8Array[] = [];
        let size = 0;
        const cancel = () => { void reader.cancel().catch(() => undefined); };
        controller.signal.addEventListener("abort", cancel, { once: true });
        try {
            for (;;) {
                const part = await reader.read();
                if (controller.signal.aborted) return fail();
                if (part.done) break;
                size += part.value.length;
                if (size > 2 * 1024 * 1024) { await reader.cancel(); return fail("NATIVE_TALENT_EVIDENCE_RESPONSE_LIMIT", 502); }
                chunks.push(part.value);
            }
        } finally { controller.signal.removeEventListener("abort", cancel); reader.releaseLock(); }
        const bytes = new Uint8Array(size);
        let offset = 0;
        for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
        let payload: unknown;
        try { payload = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); }
        catch { return fail("NATIVE_TALENT_EVIDENCE_RESPONSE_INVALID", 502); }
        if (!record(payload) || !("data" in payload)) return fail("NATIVE_TALENT_EVIDENCE_RESPONSE_INVALID", 502);
        const data = payload.data;
        if (data === null) return null;
        if (!validRow(data)) return fail("NATIVE_TALENT_EVIDENCE_RESPONSE_INVALID", 502);
        return data as unknown as TalentEvidenceRow;
    } catch (error) {
        if (error instanceof ApiError) throw error;
        return fail();
    } finally { clearTimeout(timer); }
}

const ROW_KEYS = [
    "id", "job_title", "target_start_date", "status", "version", "updated_at", "branch_id", "department_id",
    "requisition_id",
];
function validRow(value: unknown): boolean {
    if (!record(value)) return false;
    if (!validId(value.id) || !boundedText(value.job_title, 500) || !boundedText(value.status, 80)) return false;
    if (value.target_start_date !== undefined && !boundedText(value.target_start_date, 10)) return false;
    if (value.requisition_id !== undefined && !validId(value.requisition_id)) return false;
    if (!boundedText(value.branch_id, 200)) return false;
    if (value.department_id !== null && (typeof value.department_id !== "string" || value.department_id.length > 200)) return false;
    const version = Number(value.version), updatedAt = Number(value.updated_at);
    if (!Number.isSafeInteger(version) || version < 1 || !Number.isSafeInteger(updatedAt) || updatedAt < 1) return false;
    return Object.keys(value).every(key => ROW_KEYS.includes(key));
}

function responseCode(body: unknown): string | null {
    if (record(body) && record(body.error) && typeof body.error.code === "string"
        && body.error.code.length > 0 && body.error.code.length <= 100)
        return body.error.code;
    return null;
}

function application(_database: SqlDatabase) {
    const repository: TalentEvidenceRepository = {
        ...executiveEvidenceSources(_database),
        async source0(companyId: string, userId: string, assignmentId: string, providerId: "talent-recruiting-records", metric: "open-requisitions" | "application-stage-counts", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            const rows = JSON.parse(value2) as Array<{ record_id: string; record_version: number | null }>;
            await recordExecutiveEvidenceAccess({
                userId, companyId, assignmentId,
                provider: providerId, metric, action,
                fieldIds: value, queryHash,
                records: rows.map((row) => ({ recordId: row.record_id, recordVersion: Number(row.record_version ?? 0) })),
            });
            return { success: true, meta: { changes: rows.length } };
        },
    };
    return createTalentEvidenceApplication({
        repository, clock: { now: Date.now }, digest: sha256,
        talentExecutiveEvidenceRow: (...args) => nativeTalentEvidenceRow(...args),
        assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(_database, ...args)),
        currentAuthority: (...args) => fromNative(() => currentAuthority(_database, ...args)),
        metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(_database, ...args)),
    });
}
async function fromNative<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function searchTalentEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchTalentEvidence(actor, raw)); }
export async function readTalentEvidence(db: SqlDatabase, actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) { return invoke(() => application(db).readTalentEvidence(actor, metric, recordId, rawFilters)); }
export { talentEvidenceMetrics } from '@erpx/platform-runtime-server/domain/executive/talent-evidence';
