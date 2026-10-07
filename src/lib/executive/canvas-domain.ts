import { z } from "zod";

export const metricIds = ["legal-records", "legal-contract-value", "legal-exposure", "active-legal-obligations", "active-legal-holds", "active-subscriptions", "overdue-invoices", "receivables", "payables", "invoiced-sales", "cash-receipts", "close-blockers", "sales-orders", "sales-fulfillment-rate", "sales-cancellations", "sales-cancellation-value", "purchase-orders", "open-po-receipts", "invoice-match-exceptions", "headcount", "approved-leave-duration", "authorized-overtime-duration", "payroll-summary", "my-tasks", "open-operations-exceptions", "exception-resolution-time", "operations-escalations", "inventory", "available-stock", "inventory-shortages", "inventory-movements", "pos-sales", "net-pos-sales", "pos-payment-mix", "hotel-rooms", "hotel-occupancy", "hotel-adr", "hotel-revpar", "hotel-folio-balance", "hotel-night-audit-blockers", "projects", "overdue-project-milestones", "project-actual-hours", "project-posted-cost","project-dependency-blockers", "routines", "active-customers", "recent-customer-activity", "duplicate-reviews-pending", "identity-change-events", "open-support-tickets", "overdue-support-sla", "active-support-incidents", "active-assets", "asset-condition-location", "asset-retirements", "open-work-orders", "asset-downtime", "blocked-maintenance", "quality-holds", "quality-releases", "inspection-pass-rate", "opportunity-pipeline", "overdue-followups", "brand-profiles", "brand-profile-updates", "active-assignments", "pending-access-reviews", "active-organization-nodes", "pending-structure-changes", "marketing-lead-candidates", "marketing-engagement-events", "completed-manufacturing-orders", "manufacturing-orders-with-scrap", "production-cost", "planning-shortage-scenarios", "proposed-planning-recommendations", "capacity-feasible-recommendations", "open-ecommerce-carts", "completed-ecommerce-checkouts", "ecommerce-return-links", "ecommerce-fulfillment-links", "active-benefit-elections", "published-benefit-plans", "learning-completions", "overdue-required-learning", "open-requisitions", "application-stage-counts", "open-service-orders", "service-billing-releases", "field-visit-completion", "approved-agent-versions", "agent-invocation-outcomes", "collaboration-thread-activity"] as const;
export type MetricId = typeof metricIds[number];
export const breakdowns = ["default", "month", "status", "severity", "branch", "department", "level", "technician", "product", "property", "plan", "warehouse", "asset", "project", "owner", "activity-type", "movement-type", "age-band", "unit"] as const;
export type Breakdown = typeof breakdowns[number];
const english = z.string().trim().max(500).refine(v => !/[\u0E00-\u0E7F]/u.test(v), "Use English for report labels.");
export const widgetSchema = z.object({
  id: z.string().min(1).max(80), title: english.min(1), metric: z.enum(metricIds),
  kind: z.enum(["kpi", "bar", "line", "area", "column", "table", "donut", "html"]).default("kpi"),
  html: z.string().max(12000).optional(),
  css: z.string().max(12000).optional(),
  breakdown: z.enum(breakdowns).optional(),
  comparePeriod: z.enum(["previous-period"]).optional(),
  columns: z.number().int().min(2).max(12).optional(),
  height: z.number().int().min(240).max(900).optional(),
  width: z.enum(["third", "half", "full"]).default("half"),
  section: english.max(80).default("Overview"),
  description: english.default(""),
}).strict();
export const definitionSchema = z.object({
  title: english.min(1).max(120), description: english.default(""),
  theme: z.enum(["midnight", "ivory", "ocean"]).default("midnight"),
  colorMode: z.enum(["auto", "light", "dark"]).optional(),
  accent: z.enum(["violet", "blue", "emerald", "amber", "rose"]).default("violet"),
  density: z.enum(["comfortable", "compact"]).default("comfortable"),
  navigation: z.enum(["sections", "tabs"]).default("sections"),
  appearance: z.object({font:z.enum(["modern","editorial","mono"]),corners:z.enum(["soft","rounded","square"]),surface:z.enum(["flat","elevated","glass"])}).strict().optional(),
  widgets: z.array(widgetSchema).max(30).refine(w => new Set(w.map(x => x.id)).size === w.length, "Widget IDs must be unique."),
}).strict();
export const filtersSchema = z.object({
  from: z.iso.date().default("2026-01-01"), to: z.iso.date().default("2026-12-31"),
  currency: z.string().regex(/^[A-Z]{3}$/).default("THB"),
}).strict().refine(v => v.from <= v.to && (Date.parse(v.to) - Date.parse(v.from)) <= 366 * 86400000, "Choose a date range of up to one year.");
export const generateSchema = z.object({question:z.string().trim().min(3).max(8000), selectedWidget:z.string().max(80).nullable().default(null)}).strict();
export const proposalSchema = z.object({message:english.min(1), definition:definitionSchema, followUps:z.array(english.max(250)).max(3).default([])}).strict();
export const shareSchema = z.object({
  recipientId:z.string().min(1).max(180), mode:z.enum(["recipient", "distribution"]), role:z.enum(["viewer", "editor"]).default("viewer"),
  widgetIds:z.array(z.string().max(80)).min(1).max(30), expiresAt:z.number().int().positive().nullable(), filters:filtersSchema,
}).strict();
export const sharePreviewSchema=z.object({recipientId:z.string().min(1).max(180),mode:z.enum(["recipient","distribution"]),widgetIds:z.array(z.string().max(80)).min(1).max(30),filters:filtersSchema,assignmentId:z.string().min(1).max(180).nullable()}).strict();
export const transferOwnershipSchema=z.object({ownerId:z.string().min(1).max(180),assignmentId:z.string().min(1).max(180),expectedVersion:z.number().int().positive(),expectedShares:z.array(z.object({id:z.string().min(1).max(300),version:z.number().int().positive()}).strict()).max(500).refine(rows=>new Set(rows.map(row=>row.id)).size===rows.length,"Share IDs must be unique.")}).strict();
export type CanvasDefinition = z.infer<typeof definitionSchema>;
export type Widget = z.infer<typeof widgetSchema>;
export type Filters = z.infer<typeof filtersSchema>;
export type Proposal = z.infer<typeof proposalSchema>;
export type MetricInfo = { id:MetricId; label:string; description:string; unit:"money"|"count"|"percent"|"duration"|"quantity"; dateBasis:string; available:boolean; reason?:string; acceptancePending?:boolean; breakdowns?:Breakdown[]; supportsComparison?:boolean };
export type MetricDataStatus = "ready" | "empty" | "partial" | "stale" | "unavailable" | "failed";
export type MetricData = {
  schemaVersion: 2;
  metric: MetricId;
  /** Exact integer units serialized as decimal text: money in minor units, rates in hundredths of a percent, durations in milliseconds, and quantities in thousandths of the row's product unit. A null total means the rows contain incompatible units. */
  total: string | null;
  rows: { label: string; value: string; unit?: string }[];
  unit: "money" | "count" | "percent" | "duration" | "quantity";
  valueScale: 0 | 2 | 3;
  currency: string | null;
  formula: string;
  definition: string;
  scope: string;
  scopeBoundary: { level: "holding" | "company" | "branch" | "department" | "personal"; revision: string };
  referenceAt: number;
  updatedAt: number;
  sourceRevision: string;
  queryDefinitionRevision: string;
  revision: string;
  status: MetricDataStatus;
  sourceReferences: { moduleId: string; datasets: string[]; queryId: string; revision: string; referencedAt: number }[];
  error?: string;
};
export type LegacyMetricData = { metric:MetricId; total:number; rows:{label:string; value:number}[]; unit:"money"|"count"; currency:string|null; definition:string; scope:string; updatedAt:number; revision:string; error?:string };
export function normalizeMetricData(input:MetricData|LegacyMetricData):MetricData {
  if("schemaVersion" in input) return input;
  const exact=(value:number)=>{
    if(!Number.isSafeInteger(value)) throw new Error("Legacy metric values cannot be recovered exactly.");
    return String(value);
  };
  const now=input.updatedAt;
  const unavailable=!!input.error;
  return {
    schemaVersion:2, metric:input.metric, total:exact(input.total), rows:input.rows.map(row=>({label:row.label,value:exact(row.value)})),
    unit:input.unit,valueScale:input.unit==="money"?2:0,currency:input.currency,formula:input.definition,definition:input.definition,
    scope:input.scope,scopeBoundary:{level:input.scope.includes("department")?"department":input.scope.includes("branch")?"branch":"company",revision:"legacy"},
    referenceAt:now,updatedAt:now,sourceRevision:input.revision,queryDefinitionRevision:`${input.metric}:legacy`,revision:input.revision,
    status:unavailable?"unavailable":input.total===0?"empty":"ready",
    sourceReferences:[{moduleId:"legacy",datasets:[],queryId:input.revision||"legacy",revision:input.revision||"legacy",referencedAt:now}],
    ...(input.error?{error:input.error}:{}),
  };
}
export type ReportSummary = {id:string;title:string;description:string;theme:string;version:number;updatedAt:number;owner:boolean;shared:boolean;widgets:number};
export type ReportView = { id:string;definitionVersion:1;owner:boolean;canEdit:boolean;canShare:boolean;version:number;definition:CanvasDefinition;filters:Filters;lockedFilters:boolean;updatedAt:number;shares:ShareView[];revisions:{version:number;createdAt:number}[] };
export type ShareView = {id:string;recipientId:string;name:string;role:string;mode:string;widgetIds:string[];expiresAt:number|null;version:number};
export type Generation = {id:string;question:string;state:"queued"|"running"|"ready"|"failed"|"cancelled";proposal:Proposal|null;error:string|null;createdAt:number};
export function defaultFilters(now:Date):Filters { const year=now.getUTCFullYear();return {from:`${year}-01-01`,to:`${year}-12-31`,currency:"THB"}; }
export function emptyDefinition(title="Untitled report"):CanvasDefinition { return definitionSchema.parse({title,widgets:[]}); }
export function metricKey(widget:Pick<Widget,"metric"|"breakdown">) {return !widget.breakdown||widget.breakdown==="default"?widget.metric:`${widget.metric}:${widget.breakdown}`;}
export function monthBounds(value:string):{from:string;to:string}|null {
  const match=/^(\d{4})-(0[1-9]|1[0-2])$/.exec(value);
  if(!match)return null;
  const year=Number(match[1]),month=Number(match[2]);
  if(year<1)return null;
  const leap=year%4===0&&(year%100!==0||year%400===0);
  const days=[31,leap?29:28,31,30,31,30,31,31,30,31,30,31][month-1];
  return {from:`${match[1]}-${match[2]}-01`,to:`${match[1]}-${match[2]}-${String(days).padStart(2,"0")}`};
}
export function filtersForMonth(filters:Filters,month:string):Filters|null {
  const bounds=monthBounds(month);if(!bounds)return null;
  const from=filters.from>bounds.from?filters.from:bounds.from,to=filters.to<bounds.to?filters.to:bounds.to;
  if(from>to)return null;
  const result=filtersSchema.safeParse({...filters,from,to});
  return result.success?result.data:null;
}
