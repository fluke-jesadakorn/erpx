import { z } from "zod";

export const metricIds = ["legal-records", "legal-contract-value", "legal-exposure", "receivables", "payables", "invoiced-sales", "sales-orders", "purchase-orders", "headcount", "my-tasks", "inventory", "pos-sales", "hotel-rooms", "projects", "routines"] as const;
export type MetricId = typeof metricIds[number];
export const breakdowns = ["default", "month", "status", "branch", "department", "level"] as const;
export type Breakdown = typeof breakdowns[number];
const english = z.string().trim().max(500).refine(v => !/[\u0E00-\u0E7F]/u.test(v), "Use English for report labels.");
export const widgetSchema = z.object({
  id: z.string().min(1).max(80), title: english.min(1), metric: z.enum(metricIds),
  kind: z.enum(["kpi", "bar", "line", "area", "column", "table", "donut", "html"]).default("kpi"),
  html: z.string().max(12000).optional(),
  css: z.string().max(12000).optional(),
  breakdown: z.enum(breakdowns).optional(),
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
export type CanvasDefinition = z.infer<typeof definitionSchema>;
export type Widget = z.infer<typeof widgetSchema>;
export type Filters = z.infer<typeof filtersSchema>;
export type Proposal = z.infer<typeof proposalSchema>;
export type MetricInfo = { id:MetricId; label:string; description:string; unit:"money"|"count"; dateBasis:string; available:boolean; reason?:string; breakdowns?:Breakdown[] };
export type MetricData = { metric:MetricId; total:number; rows:{label:string; value:number}[]; unit:"money"|"count"; currency:string|null; definition:string; scope:string; updatedAt:number; revision:string; error?:string };
export type ReportSummary = {id:string;title:string;description:string;theme:string;version:number;updatedAt:number;owner:boolean;shared:boolean;widgets:number};
export type ReportView = { id:string;owner:boolean;canEdit:boolean;canShare:boolean;version:number;definition:CanvasDefinition;filters:Filters;lockedFilters:boolean;updatedAt:number;shares:ShareView[];revisions:{version:number;createdAt:number}[] };
export type ShareView = {id:string;recipientId:string;name:string;role:string;mode:string;widgetIds:string[];expiresAt:number|null;version:number};
export type Generation = {id:string;question:string;state:"queued"|"running"|"ready"|"failed"|"cancelled";proposal:Proposal|null;error:string|null;createdAt:number};
export function defaultFilters():Filters { const year=new Date().getUTCFullYear();return {from:`${year}-01-01`,to:`${year}-12-31`,currency:"THB"}; }
export function emptyDefinition(title="Untitled report"):CanvasDefinition { return definitionSchema.parse({title,widgets:[]}); }
export function metricKey(widget:Pick<Widget,"metric"|"breakdown">) {return !widget.breakdown||widget.breakdown==="default"?widget.metric:`${widget.metric}:${widget.breakdown}`;}
