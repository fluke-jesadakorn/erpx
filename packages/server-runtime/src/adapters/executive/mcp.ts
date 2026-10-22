import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import "server-only";
import { randomUUID } from "node:crypto";
import { createMcpHandler, McpServer, type CallToolResult } from "@modelcontextprotocol/server";
import { z } from "zod";
import { defaultFilters } from "@erpx/server-runtime/public/rules/canvas-domain";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { runtimeEnv } from "../../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { consumeExecutiveRateLimit, findExecutiveToken, recheckExecutivePrincipal, resolveExecutivePrincipal, type ExecutivePrincipal, type TokenRow } from "@erpx/server-runtime/public/executive/connections";
import { reportAccess, reportData } from "@erpx/server-runtime/public/executive/reports";
import { metricCatalogFor, queryMetric } from "@erpx/server-runtime/public/executive/metrics";
import { createExecutiveTools, type ExecutiveMcpUnit } from "@erpx/server-runtime/application/executive/mcp";
import { executiveToolSchemas, type ToolName } from "@erpx/server-runtime/domain/executive/mcp";
export { executiveToolSchemas, metricProjection } from "@erpx/server-runtime/domain/executive/mcp";
const BODY_BYTES = 32 * 1024, TOOL_RESULT_BYTES = 128 * 1024;
const annotations = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false };
function executiveMcpUnit(db: SqlDatabase): ExecutiveMcpUnit {
    return {
        dashboards: (actor, cursor, now, limit) => db.prepare(`SELECT r.id FROM executive_reports r WHERE r.company_id=? AND r.id>?
      AND (r.owner_id=? OR EXISTS(SELECT 1 FROM executive_report_shares s WHERE s.report_id=r.id AND s.company_id=r.company_id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?)))
      ORDER BY r.id LIMIT ?`).bind(actor.companyId, cursor, actor.userId, actor.userId, now, limit).all<{
            id: string;
        }>(),
        access: (actor, id) => reportAccess(db, actor, id), data: (actor, id, filters) => reportData(db, actor, id, filters), catalog: actor => metricCatalogFor(db, actor), metric: (actor, ...args) => queryMetric(db, actor, ...args)
    };
}
const tools = createExecutiveTools({ clock: { now: Date.now }, bytes: value => Buffer.byteLength(value), defaultFilters, error: (...args) => new ApiError(...args), errorStatus: error => error instanceof ApiError ? error.status : undefined });
export const boundedExecutiveResult = tools.boundedExecutiveResult;
export function executeExecutiveTool(p: ExecutivePrincipal, name: ToolName, raw: unknown) { return tools.executeExecutiveTool({ actor: p.actor, db: executiveMcpUnit(p.db) }, name, raw); }
async function readBody(request: Request) {
    if (!request.headers.get("content-type")?.toLowerCase().startsWith("application/json"))
        throw new ApiError(415, "CONTENT_TYPE_REQUIRED", "Use application/json.");
    const reader = request.body?.getReader();
    if (!reader)
        throw new ApiError(400, "INVALID_JSON", "A JSON-RPC body is required.");
    let bytes = 0, text = "";
    const decoder = new TextDecoder();
    while (true) {
        const chunk = await reader.read();
        if (chunk.done)
            break;
        bytes += chunk.value.byteLength;
        if (bytes > BODY_BYTES) {
            await reader.cancel();
            throw new ApiError(413, "BODY_TOO_LARGE", "MCP requests are limited to 32 KiB.");
        }
        text += decoder.decode(chunk.value, { stream: true });
    }
    try {
        return JSON.parse(text + decoder.decode()) as unknown;
    }
    catch {
        throw new ApiError(400, "INVALID_JSON", "Use a valid JSON-RPC request.");
    }
}
function failure(error: unknown) {
    return error instanceof ApiError ? error : new ApiError(503, "EXECUTIVE_UNAVAILABLE", "Executive MCP is temporarily unavailable. Retry later.");
}
function httpFailure(e: ApiError) {
    return Response.json(canonicalDestinationLinks({ jsonrpc: "2.0", id: null, error: { code: -32000, message: e.message, data: { code: e.code } } }), { status: e.status, headers: { "Cache-Control": "private, no-store", ...(e.status === 401 ? { "WWW-Authenticate": 'Bearer realm="ERPX Executive"' } : {}), ...(e.status === 429 ? { "Retry-After": String(60 - Math.floor(Date.now() / 1000) % 60) } : {}) } });
}
export async function handleExecutiveMcp(request: Request) {
    const started = Date.now();
    let control: SqlDatabase | undefined, token: TokenRow | undefined, action = "request", outcome: "success" | "denied" | "failed" = "success", code = "OK";
    let response: Response;
    try {
        control = (await runtimeEnv()).CONTROL_DB;
        const origin = request.headers.get("origin");
        if (origin && origin !== requestPublicOrigin(request))
            throw new ApiError(403, "ORIGIN_FORBIDDEN", "This MCP origin is not allowed.");
        token = await findExecutiveToken(control, request.headers.get("authorization"));
        // Expired/revoked attempts still have attribution, but never reach a business read.
        const p = await resolveExecutivePrincipal(control, token);
        await consumeExecutiveRateLimit(control, token);
        if (request.method !== "POST")
            throw new ApiError(405, "METHOD_NOT_ALLOWED", "Use Streamable HTTP POST. This endpoint holds no sessions or subscription streams.");
        const raw = await readBody(request);
        if (!raw || typeof raw !== "object" || Array.isArray(raw))
            throw new ApiError(400, "INVALID_REQUEST", "Send one JSON-RPC request at a time.");
        const rpc = raw as {
            method?: unknown;
            params?: {
                name?: unknown;
                arguments?: unknown;
            };
        };
        const toolName = rpc.method === "tools/call" && typeof rpc.params?.name === "string" && Object.hasOwn(executiveToolSchemas, rpc.params.name) ? rpc.params.name as ToolName : undefined;
        action = toolName ?? (["initialize", "server/discover", "tools/list", "ping", "notifications/initialized"].includes(String(rpc.method)) ? String(rpc.method) : "unknown");
        if (rpc.method === "tools/call" && (!toolName || !executiveToolSchemas[toolName].safeParse(rpc.params?.arguments ?? {}).success)) {
            outcome = "denied";
            code = "INVALID_TOOL_ARGUMENTS";
        }
        const handler = createMcpHandler(() => {
            const server = new McpServer({ name: "erpx-executive", version: "1.0.0" });
            const register = (name: ToolName, description: string, schema: z.ZodObject) => server.registerTool(name, { description, inputSchema: schema, annotations }, async (args: unknown): Promise<CallToolResult> => {
                try {
                    await recheckExecutivePrincipal(control!, p);
                    const result = boundedExecutiveResult(await executeExecutiveTool(p, name, args));
                    await recheckExecutivePrincipal(control!, p);
                    return { content: [{ type: "text", text: JSON.stringify(result) }], structuredContent: result };
                }
                catch (error) {
                    const e = failure(error);
                    outcome = e.status >= 500 ? "failed" : "denied";
                    code = e.code;
                    return { isError: true, content: [{ type: "text", text: JSON.stringify({ error: { code: e.code, message: e.message } }) }] };
                }
            });
            register("list_dashboards", "List owned and currently shared Executive dashboards. Use nextCursor to continue.", executiveToolSchemas.list_dashboards);
            register("get_dashboard", "Read dashboard title, approved filters and analytical widget metadata.", executiveToolSchemas.get_dashboard);
            register("get_dashboard_data", "Read exact dashboard metrics and comparisons using existing recipient/distribution share rules. Shared dates can only be narrowed. When valueAvailable is false, total is a contract sentinel; never interpret it as a measured zero.", executiveToolSchemas.get_dashboard_data);
            register("list_metrics", "List Executive KPI definitions, supported breakdowns, availability and pending acceptance for the token's current role and scope.", executiveToolSchemas.list_metrics);
            register("query_metric", "Read an authorized KPI for a date interval and supported breakdown. Exact integer totals are decimal strings. A report share does not grant additional KPI access.", executiveToolSchemas.query_metric);
            return server;
        }, { responseMode: "auto", maxRequestBodySize: BODY_BYTES, maxSubscriptions: 0 });
        try {
            const result = await handler.fetch(request, { parsedBody: raw });
            const body = await result.text(); // Finish the SDK stream before disposing the per-request server.
            if (Buffer.byteLength(body) > 2 * TOOL_RESULT_BYTES + 8192)
                throw new ApiError(413, "RESULT_TOO_LARGE", "The MCP response exceeds its size limit.");
            await recheckExecutivePrincipal(control, p);
            if (result.status >= 400) {
                outcome = "denied";
                code = "PROTOCOL_REJECTED";
            }
            else if (outcome === "success" && body) {
                // SDK validation failures may use HTTP 200 JSON or legacy SSE; audit the protocol outcome too.
                const wire = result.headers.get("content-type")?.includes("text/event-stream") ? body.split("\n").filter(line => line.startsWith("data: ")).at(-1)?.slice(6) : body;
                if (wire)
                    try {
                        const message = JSON.parse(wire);
                        if (message.error || message.result?.isError) {
                            outcome = "denied";
                            code = "PROTOCOL_REJECTED";
                        }
                    }
                    catch { /* Non-result notifications carry no business data. */ }
            }
            response = new Response(body || null, { status: result.status, headers: result.headers });
            response.headers.set("Cache-Control", "private, no-store");
            await control.prepare("UPDATE executive_external_tokens SET last_used_at=? WHERE id=? AND revoked_at IS NULL").bind(Date.now(), token.id).run();
        }
        finally {
            await handler.close();
        }
    }
    catch (error) {
        const e = failure(error);
        outcome = e.status >= 500 ? "failed" : "denied";
        code = e.code;
        response = httpFailure(e);
    }
    try {
        if (!control)
            throw new Error("CONTROL_UNAVAILABLE");
        await control.prepare(`INSERT INTO executive_mcp_audit(id,token_id,user_id,company_id,assignment_id,branch_id,department_id,action,outcome,code,started_at,duration_ms)
      VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`).bind(randomUUID(), token?.id ?? null, token?.user_id ?? null, token?.company_id ?? null, token?.assignment_id ?? null, token?.branch_id ?? null, token?.department_id ?? null, action, outcome, code, started, Date.now() - started).run();
    }
    catch {
        return httpFailure(new ApiError(503, "EXECUTIVE_UNAVAILABLE", "Executive MCP is temporarily unavailable. Retry later."));
    }
    return response;
}
