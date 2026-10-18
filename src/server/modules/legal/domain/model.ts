import type {LegalPurchaseControls} from './purchases';
import { z } from "zod";
export const kinds = [
  "request",
  "matter",
  "contract",
  "obligation",
  "compliance",
  "license",
  "corporate",
  "ip",
  "privacy",
  "counsel",
  "template",
  "rule",
] as const;
export type LegalKind = (typeof kinds)[number];
export const labels: Record<LegalKind, string> = {
  request: "Requests & advice",
  matter: "Cases & disputes",
  contract: "Contracts",
  obligation: "Obligations",
  compliance: "Compliance",
  license: "Licenses",
  corporate: "Corporate records",
  ip: "Intellectual property",
  privacy: "Privacy",
  counsel: "Outside counsel",
  template: "Templates & clauses",
  rule: "Jurisdiction rules",
};
export type Field = {
  key: string;
  label: string;
  type?: "date" | "number" | "textarea";
  required?: boolean;
};
export const fields: Record<LegalKind, Field[]> = {
  request: [
    { key: "requestType", label: "Request type", required: true },
    {
      key: "question",
      label: "Question / advice requested",
      type: "textarea",
      required: true,
    },
    { key: "advice", label: "Legal advice", type: "textarea" },
  ],
  matter: [
    { key: "caseNumber", label: "Case number" },
    { key: "authority", label: "Court / authority" },
    { key: "parties", label: "Parties", type: "textarea", required: true },
    { key: "claim", label: "Claim and relief", type: "textarea" },
    {
      key: "exposure",
      label: "Exposure (minor currency units)",
      type: "number",
    },
    { key: "strategy", label: "Strategy / orders / appeal", type: "textarea" },
    { key: "counsel", label: "Counsel" },
    { key: "outcome", label: "Settlement / enforcement", type: "textarea" },
  ],
  contract: [
    { key: "contractType", label: "Contract type", required: true },
    {
      key: "parties",
      label: "Parties and signatory authority",
      type: "textarea",
      required: true,
    },
    { key: "value", label: "Value (minor currency units)", type: "number" },
    { key: "effectiveDate", label: "Effective date", type: "date" },
    { key: "endDate", label: "Expiry date", type: "date" },
    {
      key: "renewalTerms",
      label: "Renewal / termination terms",
      type: "textarea",
    },
  ],
  obligation: [
    {
      key: "obligationType",
      label: "Delivery / payment / guarantee / insurance",
      required: true,
    },
    {
      key: "requirement",
      label: "Requirement",
      type: "textarea",
      required: true,
    },
    {
      key: "completionEvidence",
      label: "Completion evidence",
      type: "textarea",
    },
    { key: "escalationOwner", label: "Escalation owner user ID" },
  ],
  compliance: [
    { key: "source", label: "Official source URL", required: true },
    {
      key: "applicability",
      label: "Applicability assessment",
      type: "textarea",
      required: true,
    },
    { key: "control", label: "Control and evidence", type: "textarea" },
    { key: "finding", label: "Finding / corrective action", type: "textarea" },
  ],
  license: [
    { key: "licenseNumber", label: "License number" },
    { key: "issuer", label: "Issuing authority", required: true },
    { key: "conditions", label: "Conditions", type: "textarea" },
    { key: "issuedDate", label: "Issued date", type: "date" },
    { key: "renewalEvidence", label: "Renewal evidence", type: "textarea" },
  ],
  corporate: [
    {
      key: "recordType",
      label: "Resolution / minutes / power of attorney",
      required: true,
    },
    {
      key: "authority",
      label: "Authorized person and limits",
      type: "textarea",
    },
    { key: "meetingDate", label: "Meeting date", type: "date" },
    { key: "resolution", label: "Resolution", type: "textarea" },
  ],
  ip: [
    {
      key: "assetType",
      label: "Trademark / copyright / patent / domain",
      required: true,
    },
    { key: "rightsOwner", label: "Rights owner", required: true },
    { key: "applicationNumber", label: "Application / registration number" },
    { key: "classes", label: "Goods / services classes" },
    {
      key: "licenseTerms",
      label: "Licensing / infringement",
      type: "textarea",
    },
  ],
  privacy: [
    {
      key: "recordType",
      label: "Processing / subject request / breach",
      required: true,
    },
    {
      key: "purpose",
      label: "Purpose and lawful basis",
      type: "textarea",
      required: true,
    },
    {
      key: "dataCategories",
      label: "Data categories / subjects",
      type: "textarea",
    },
    { key: "recipients", label: "Recipients / transfers", type: "textarea" },
    {
      key: "response",
      label: "Assessment / response evidence",
      type: "textarea",
    },
  ],
  counsel: [
    { key: "firm", label: "Firm / lawyer", required: true },
    {
      key: "conflicts",
      label: "Conflict check and outcome",
      type: "textarea",
      required: true,
    },
    { key: "budget", label: "Budget (minor currency units)", type: "number" },
    { key: "minutes", label: "Time worked (minutes)", type: "number" },
    { key: "fees", label: "Fees (minor currency units)", type: "number" },
    { key: "financeReference", label: "Finance request reference" },
  ],
  template: [
    { key: "templateType", label: "Template / clause", required: true },
    { key: "variables", label: "Variables (one per line)", type: "textarea" },
    {
      key: "body",
      label: "Document / clause text",
      type: "textarea",
      required: true,
    },
    { key: "alternative", label: "Alternative wording", type: "textarea" },
  ],
  rule: [
    { key: "source", label: "Official source URL", required: true },
    {
      key: "effectiveDate",
      label: "Effective date (required before activation)",
      type: "date",
    },
    {
      key: "applicability",
      label: "Applicability",
      type: "textarea",
      required: true,
    },
    {
      key: "procedure",
      label: "Procedure / calculation specification",
      type: "textarea",
      required: true,
    },
    { key: "reviewNote", label: "Review evidence", type: "textarea" },
  ],
};
export const dateSchema = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/)
  .refine(
    (s) =>
      !Number.isNaN(Date.parse(s)) &&
      new Date(s).toISOString().slice(0, 10) === s,
    "Use a valid calendar date.",
  );
export const itemSchema = z
  .object({
    kind: z.enum(kinds),
    title: z.string().trim().min(2).max(240),
    description: z.string().max(20000).default(""),
    classification: z
      .enum(["internal", "confidential", "restricted"])
      .default("confidential"),
    country: z
      .string()
      .regex(/^[A-Z]{2}$/)
      .default("TH"),
    jurisdiction: z.string().max(200).default("Thailand"),
    language: z.string().min(2).max(40).default("th"),
    currency: z
      .string()
      .regex(/^[A-Z]{3}$/)
      .default("THB"),
    timezone: z
      .string()
      .refine((s) => {
        try {
          new Intl.DateTimeFormat("en", { timeZone: s });
          return true;
        } catch {
          return false;
        }
      }, "Invalid timezone")
      .default("Asia/Bangkok"),
    priority: z.enum(["normal", "high", "urgent"]).default("normal"),
    dueDate: dateSchema.nullable().default(null),
    ownerId: z.string().min(1).max(200),
    data: z
      .record(
        z.string(),
        z.union([z.string().max(50000), z.number().nonnegative().safe()]),
      )
      .default({}),
  })
  .strict();
export type ItemInput = z.infer<typeof itemSchema>;
export function parseItem(input: unknown): ItemInput {
  const x = itemSchema.parse(input);
  const allowed = fields[x.kind];
  for (const key of Object.keys(x.data))
    if (!allowed.some((f) => f.key === key))
      throw new Error(`Unknown ${x.kind} field: ${key}`);
  for (const f of allowed) {
    const v = x.data[f.key];
    if (f.required && (v === undefined || String(v).trim() === ""))
      throw new Error(`${f.label} is required.`);
    if (v !== undefined && v !== "") {
      if (f.type === "number") z.number().nonnegative().safe().parse(v);
      else z.string().parse(v);
      if (f.type === "date") dateSchema.parse(v);
    }
  }
  if (
    x.kind === "contract" &&
    x.data.effectiveDate &&
    x.data.endDate &&
    x.data.endDate < x.data.effectiveDate
  )
    throw new Error("Expiry must follow the effective date.");
  if (["rule", "compliance"].includes(x.kind)) {
    const u = new URL(String(x.data.source));
    if (u.protocol !== "https:") throw new Error("Use an HTTPS source URL.");
  }
  return x;
}
export type LegalItem = {
  id: string;
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  kind: LegalKind;
  title: string;
  description: string;
  classification: ItemInput["classification"];
  country: string;
  jurisdiction: string;
  language: string;
  currency: string;
  timezone: string;
  priority: string;
  due_date: string | null;
  owner_id: string;
  created_by: string;
  state: string;
  version: number;
  created_at: number;
  updated_at: number;
  data: ItemInput["data"];
};
export const transitions: Record<string, string[]> = {
  draft: ["review", "cancelled"],
  review: ["draft", "approval", "rejected"],
  approval: ["draft", "approved", "rejected"],
  approved: ["active", "draft"],
  active: ["completed", "closed", "expired", "terminated"],
  completed: ["active", "archived"],
  closed: ["active", "archived"],
  expired: ["draft", "archived"],
  terminated: ["archived"],
  rejected: ["draft", "archived"],
  cancelled: ["draft", "archived"],
  archived: [],
};
export function allowedTransitions(kind: LegalKind, state: string) {
  if (kind === "contract") {
    if (state === "approval") return ["draft", "approved", "rejected"];
    if (state === "approved") return ["draft"];
    if (state === "signing") return ["cancelled"];
    if (state === "executed") return ["active"];
    if (state === "active") return ["expired", "terminated"];
  }
  return transitions[state] ?? [];
}
export type DocumentVersion = {
  id: string;
  item_id: string;
  version: number;
  name: string;
  media_type: string;
  checksum: string;
  bytes: number;
  body: string;
  created_by: string;
  created_at: number;
  locked: boolean;
  object_key: string | null;
  attachment_link_id?: string | null;
  attachment_source_id?: string | null;
};
export type LegalEvent = {
  snapshot?: string | null;
  id: string;
  action: string;
  actor_id: string;
  note: string;
  created_at: number;
  version: number;
};
export type LegalDetail = {
  purchases?:LegalPurchaseControls;
  submissions: {
    id: string;
    name: string;
    checksum: string;
    created_by: string;
    created_at: number;
  }[];
  shares: {
    id: string;
    email: string;
    document_id: string;
    user_id: string | null;
    expires_at: number;
    revoked_at: number | null;
  }[];
  item: LegalItem;
  documents: DocumentVersion[];
  events: LegalEvent[];
  members: { user_id: string; access: string }[];
  holds: { id: string; reason: string; state: string; direct: boolean; acknowledged_at:number|null }[];
  links: { target_id: string; relation: string; title: string }[];
  comments: {
    id: string;
    body: string;
    visibility: string;
    created_by: string;
    created_at: number;
  }[];
  envelopes: {
    id: string;
    state: string;
    document_id: string;
    expires_at: number;
    consent: string;
    signers: {
      user_id: string;
      ordinal: number;
      state: string;
      signed_at: number | null;
    }[];
  }[];
};
export type LegalData = {
  purchases?:LegalPurchaseControls;
  workContext?: {companyId:string;branchId:string|null;departmentId:string|null;assignmentId:string;userId:string};
  enabled: boolean;
  items: LegalItem[];
  total: number;
  page: number;
  counts: { state: string; count: number }[];
  permissions: string[];
  userId: string;
  people: { id: string; name: string }[];
};
export const executedCopySchema=z.object({
  approvedDocumentId:z.string().min(1),
  signedDate:dateSchema,
  parties:z.array(z.string().trim().min(1).max(300)).min(2).max(30),
  signers:z.array(z.object({name:z.string().trim().min(1).max(300),party:z.string().trim().min(1).max(300),authorityDocumentIds:z.array(z.string().min(1)).min(1).max(20)}).strict()).min(1).max(30),
  declaredPageCount:z.number().int().min(1).max(1000),
  notes:z.string().max(4000).default(""),
}).strict().refine(x=>x.signers.every(s=>x.parties.includes(s.party)),"Every signer must belong to a named party.");
export type ExecutedCopyInput=z.infer<typeof executedCopySchema>;
export type ExecutedCopy={id:string;item_id:string;document_id:string;approved_cycle:number;approved_document_id:string;approved_hash:string;executed_hash:string;metadata:ExecutedCopyInput;state:"pending_review"|"changes_required"|"confirmed";imported_by:string;imported_at:number;reviewed_by:string|null;reviewed_at:number|null;version:number};
