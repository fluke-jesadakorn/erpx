import "server-only";
import samples from "./module-samples.json";
import { findModule } from "../module-registry";
import { capabilitySamples } from "./capability-samples";
export type PreviewField = {
  key: string;
  label: string;
  type: string;
  value: string | number | boolean;
  options: string[];
};
export type PreviewStep = {
  status: string;
  action: string;
  next: string;
  help: string;
  gate: string;
  handoff: string;
};
export type PreviewRecord = {
  id: string;
  title: string;
  description: string;
  status: string;
  fields: PreviewField[];
  source: "sample";
};
type Sample = {
  fields: PreviewField[];
  steps: PreviewStep[];
  rows: string[][];
  relations: string[][];
};
const data = samples as unknown as Record<string, Sample>;
export function getModuleSamples(id: string, sectionId?: string) {
  const sample = data[id];
  if (!sample) return undefined;
  const records: PreviewRecord[] = sample.rows.map((row) => {
    // The original prototype used a separate workflow draft; reconcile its visible fields
    // with each fixture's record summary so previews never contradict themselves.
    const amount = row[2].match(/\$([\d,]+(?:\.\d+)?)(k)?/i);
    const fields = sample.fields.map((field) => {
      let value = field.value;
      if (
        amount &&
        [
          "amount",
          "debit",
          "credit",
          "value",
          "total",
          "budget",
          "salary",
        ].includes(field.key)
      )
        value = Number(amount[1].replaceAll(",", "")) * (amount[2] ? 1000 : 1);
      if (id === "sales" && field.key === "customer") value = row[1];
      if (field.key === "notes") value = row[2];
      const terms = row[2].match(/Net (\d+)/i);
      if (id === "sales" && field.key === "terms" && terms)
        value = Number(terms[1]);
      return { ...field, value };
    });
    return {
      id: row[0],
      title: row[1],
      description: row[2],
      status: row[3],
      fields,
      source: "sample",
    };
  });
  const relations = sample.relations.flatMap(([target, label]) => {
    const [moduleId, recordId] = target.split("/");
    const row = data[moduleId]?.rows.find((item) => item[0] === recordId);
    return row && findModule(moduleId)
      ? [{ moduleId, recordId, label, title: row[1] }]
      : [];
  });
  const definition = findModule(id)!;
  const section = definition.sections.find((item) => item.id === sectionId);
  const needsSpecificPrimary = [
    "identity_access",
    "pos",
    "inventory",
    "ecommerce",
    "benefits",
    "learning",
    "talent",
    "agent",
    "commercial",
    "platform_administration",
  ].includes(id);
  // Preserve original fixture identifiers for the representative workflow. Other
  // capabilities use their own appropriate fields, rather than relabeling that workflow.
  if (
    section &&
    ![
      ...(needsSpecificPrimary ? [] : [definition.primary[0]]),
      "overview",
      "records",
      "activity",
    ].includes(section.id)
  )
    return { ...capabilitySamples(definition, section), relations };
  return { records, fields: sample.fields, steps: sample.steps, relations };
}
