import { z } from "zod";
import type { LegalKind } from "./model";
export type FeatureField = {
  key: string;
  label: string;
  type?: "date" | "number" | "textarea";
  required?: boolean;
};
export type FeatureSpec = {
  label: string;
  kinds: LegalKind[];
  table: string;
  fields: FeatureField[];
  states: Record<string, string[]>;
  initial: string;
  evidenceStates: string[];
  independentStates: string[];
};
const f = (
  key: string,
  label: string,
  type: FeatureField["type"] = "textarea",
  required = true,
): FeatureField => ({ key, label, type, required });
export const featureSpecs = {
  case_event: {
    label: "Hearings, filings and case actions",
    kinds: ["matter"],
    table: "legal_case_events",
    fields: [
      f(
        "eventType",
        "Hearing / filing / order / appeal / settlement / enforcement",
      ),
      f("authority", "Court or authority"),
      f("details", "Action and requirements"),
      f("evidence", "Order, filing receipt or outcome", "textarea", false),
    ],
    initial: "planned",
    states: {
      planned: ["in_progress", "cancelled"],
      in_progress: ["review", "completed", "cancelled"],
      review: ["approved", "in_progress", "rejected"],
      approved: ["completed"],
      completed: [],
      cancelled: [],
      rejected: ["planned"],
    },
    evidenceStates: ["review", "completed"],
    independentStates: ["approved"],
  },
  compliance_action: {
    label: "Controls, findings and remediation",
    kinds: ["compliance"],
    table: "legal_compliance_actions",
    fields: [
      f("control", "Requirement and control"),
      f("assessment", "Applicability, assessment and finding"),
      f("remediation", "Corrective action", "textarea", false),
      f("evidence", "Audit or remediation evidence", "textarea", false),
    ],
    initial: "open",
    states: {
      open: ["remediation", "review", "not_applicable"],
      remediation: ["review"],
      review: ["verified", "remediation"],
      verified: ["closed"],
      not_applicable: ["open"],
      closed: ["open"],
    },
    evidenceStates: ["review", "verified", "not_applicable"],
    independentStates: ["verified"],
  },
  privacy_case: {
    label: "Data-subject requests and breaches",
    kinds: ["privacy"],
    table: "legal_privacy_cases",
    fields: [
      f("caseType", "subject_request / breach / processing"),
      f("basis", "Purpose, legal basis and applicable rule"),
      f("scope", "Data, subjects, systems and recipients"),
      f(
        "assessment",
        "Identity verification or impact assessment",
        "textarea",
        false,
      ),
      f(
        "response",
        "Response, notification and remediation",
        "textarea",
        false,
      ),
      f("evidence", "Response / notification receipt", "textarea", false),
    ],
    initial: "received",
    states: {
      received: ["assessed", "rejected"],
      assessed: ["in_progress", "review"],
      in_progress: ["review"],
      review: ["approved", "in_progress"],
      approved: ["responded"],
      responded: ["closed"],
      closed: ["received"],
      rejected: [],
    },
    evidenceStates: ["assessed", "responded", "closed"],
    independentStates: ["approved"],
  },
  corporate_action: {
    label: "Corporate filings and permit renewals",
    kinds: ["corporate", "license"],
    table: "legal_corporate_actions",
    fields: [
      f("actionType", "Meeting / resolution / filing / renewal"),
      f("authority", "Authority and submission channel"),
      f("requirements", "Required documents and conditions"),
      f(
        "evidence",
        "Minutes, resolution, receipt or renewed permit",
        "textarea",
        false,
      ),
    ],
    initial: "preparing",
    states: {
      preparing: ["review", "cancelled"],
      review: ["approved", "preparing", "rejected"],
      approved: ["submitted"],
      submitted: ["completed", "preparing"],
      completed: [],
      cancelled: [],
      rejected: ["preparing"],
    },
    evidenceStates: ["review", "submitted", "completed"],
    independentStates: ["approved"],
  },
  ip_action: {
    label: "IP prosecution, renewal and infringement",
    kinds: ["ip"],
    table: "legal_ip_actions",
    fields: [
      f("actionType", "Application / response / renewal / infringement"),
      f("authority", "Jurisdiction and authority"),
      f("requirements", "Classes, required action and grounds"),
      f(
        "evidence",
        "Filing, payment, decision or settlement evidence",
        "textarea",
        false,
      ),
    ],
    initial: "preparing",
    states: {
      preparing: ["review", "cancelled"],
      review: ["approved", "preparing", "rejected"],
      approved: ["filed"],
      filed: ["examined", "completed", "rejected"],
      examined: ["preparing", "completed", "rejected"],
      completed: [],
      cancelled: [],
      rejected: ["preparing"],
    },
    evidenceStates: ["filed", "completed"],
    independentStates: ["approved"],
  },
  counsel_entry: {
    label: "Conflict review, time and fees",
    kinds: ["counsel"],
    table: "legal_counsel_entries",
    fields: [
      f("entryType", "conflict / time / fee / invoice"),
      f("description", "Work, related parties and conflict search"),
      f("minutes", "Minutes", "number", false),
      f("amount", "Amount in minor currency units", "number", false),
      f(
        "evidence",
        "Timesheet, invoice or conflict review evidence",
        "textarea",
        false,
      ),
    ],
    initial: "draft",
    states: {
      draft: ["submitted", "cancelled"],
      submitted: ["approved", "rejected", "draft"],
      approved: ["billed", "closed"],
      billed: ["closed"],
      rejected: ["draft"],
      closed: [],
      cancelled: [],
    },
    evidenceStates: ["submitted", "billed"],
    independentStates: ["approved"],
  },
  schedule: {
    label: "Commitments and scheduled deliverables",
    kinds: ["contract", "obligation", "license", "ip", "matter"],
    table: "legal_obligation_schedule",
    fields: [
      f(
        "scheduleType",
        "Delivery / payment / notice / guarantee / insurance / renewal",
      ),
      f("requirement", "Required action"),
      f("amount", "Amount in minor currency units", "number", false),
      f("evidence", "Completion evidence", "textarea", false),
    ],
    initial: "pending",
    states: {
      pending: ["in_progress", "waiver_review"],
      in_progress: ["completed", "waiver_review"],
      waiver_review: ["waived", "pending"],
      waived: [],
      completed: ["in_progress"],
    },
    evidenceStates: ["completed", "waiver_review"],
    independentStates: ["waived"],
  },
  custody: {
    label: "Evidence custody and transfers",
    kinds: [
      "request",
      "matter",
      "contract",
      "compliance",
      "privacy",
      "corporate",
      "license",
      "ip",
      "counsel",
      "obligation",
    ],
    table: "legal_custody_events",
    fields: [
      f("documentId", "Document version ID"),
      f("from", "Transferor and location"),
      f("to", "Recipient and location"),
      f("purpose", "Purpose and condition"),
      f("evidence", "Receipt and integrity observation", "textarea", false),
    ],
    initial: "prepared",
    states: {
      prepared: ["transferred", "cancelled"],
      transferred: ["received", "disputed"],
      received: [],
      disputed: ["received"],
      cancelled: [],
    },
    evidenceStates: ["transferred", "received", "disputed"],
    independentStates: ["received"],
  },
  policy_ack: {
    label: "Policy acknowledgement",
    kinds: ["compliance", "privacy", "corporate"],
    table: "legal_policy_acknowledgements",
    fields: [
      f("documentId", "Policy document version ID"),
      f("instruction", "Acknowledgement instruction"),
      f("evidence", "Acknowledgement statement", "textarea", false),
    ],
    initial: "pending",
    states: { pending: ["acknowledged"], acknowledged: [] },
    evidenceStates: ["acknowledged"],
    independentStates: [],
  },
  intake: {
    label: "Information requests and responses",
    kinds: ["request"],
    table: "legal_intake_questions",
    fields: [
      f("question", "Information required"),
      f("response", "Response", "textarea", false),
      f("evidence", "Response evidence", "textarea", false),
    ],
    initial: "requested",
    states: {
      requested: ["responded", "cancelled"],
      responded: ["accepted", "requested"],
      accepted: [],
      cancelled: [],
    },
    evidenceStates: ["responded"],
    independentStates: [],
  },
} satisfies Record<string, FeatureSpec>;
export type FeatureKind = keyof typeof featureSpecs;
export type FeatureRow = {
  id: string;
  item_id: string;
  category: FeatureKind;
  title: string;
  owner_id: string;
  due_date: string | null;
  state: string;
  version: number;
  data: Record<string, string>;
  created_by: string;
  updated_by: string;
  updated_at: number;
};
export const featureKinds = Object.keys(featureSpecs) as [
  FeatureKind,
  ...FeatureKind[],
];
const date = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/)
  .refine((s) => {
    const d = new Date(s + "T00:00:00Z");
    return !Number.isNaN(d.valueOf()) && d.toISOString().slice(0, 10) === s;
  }, "Invalid calendar date");
export const featureInput = z.object({
  category: z.enum(featureKinds),
  title: z.string().trim().min(3).max(200),
  ownerId: z.string().min(1),
  dueDate: date.nullable().default(null),
  data: z.record(z.string(), z.string().max(12000)),
});
export function parseFeature(raw: unknown) {
  const x = featureInput.parse(raw),
    spec: FeatureSpec = featureSpecs[x.category];
  for (const key of Object.keys(x.data))
    if (!spec.fields.some((f) => f.key === key))
      throw new Error(`Unknown field: ${key}`);
  for (const field of spec.fields) {
    const value = x.data[field.key];
    if (field.required && !value?.trim())
      throw new Error(`${field.label} is required`);
    if (value && field.type === "number" && !/^\d{1,14}$/.test(value))
      throw new Error(`${field.label} must be nonnegative whole units`);
  }
  return x;
}
