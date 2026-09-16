// Import data declarations only; never execute the prototype's browser command layer.
import fs from "node:fs";
import vm from "node:vm";
const context = vm.createContext({ window: {} });
for (const name of ["modules"])
  vm.runInContext(
    fs.readFileSync(`Designed/mockup/${name}.js`, "utf8"),
    context,
  );
const capabilitySource = fs.readFileSync(
  "Designed/mockup/capabilities.js",
  "utf8",
);
vm.runInContext(
  capabilitySource.slice(
    0,
    capabilitySource.indexOf("function capabilitySlug"),
  ) + "\nwindow.manifest = CAPABILITY_MANIFEST;",
  context,
);
const {
  ERPX_MODULES: { MODULES: specs },
  manifest,
} = context.window;
const slug = (value) =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/-$/, "");
function kind(module, label) {
  const value = `${module} ${label}`.toLowerCase();
  if (
    /availability|reports|summary|summaries|aging|comparison|costs|genealogy|readiness|capacity|shortages/.test(
      value,
    )
  )
    return "analytics";
  if (
    /journal|payable|payment|receivable|bank|close|opening balance|invoice/.test(
      value,
    )
  )
    return "ledger";
  if (/schedule|timeline|history|activity|audit|downtime/.test(value))
    return "timeline";
  if (
    /pipeline|application|wbs|dependenc|handoff|queue|fulfillment|checkout|return/.test(
      value,
    )
  )
    return "board";
  if (
    /document|attachment|content|preview|listing|catalog|course|plan/.test(
      value,
    )
  )
    return "library";
  if (
    /connector|infrastructure|backup|projection|migration|domain|provider|provision|health/.test(
      value,
    )
  )
    return "health";
  if (
    /account|profile|people|principal|employee|candidate|dependent|customer|organization tree/.test(
      value,
    )
  )
    return "directory";
  if (/thread|comment|mention|conversation|private session/.test(value))
    return "conversation";
  return "workflow";
}
const registry = Object.keys(manifest).map((id) => {
  const spec = specs[id];
  const sections = spec.tabs.map((label) => ({
    id: slug(label),
    label,
    description: spec.focus,
    kind: kind(id, label),
    capability: false,
  }));
  for (const [label, description] of manifest[id]) {
    const existing = sections.find((section) => section.id === slug(label));
    if (existing) Object.assign(existing, { description, capability: true });
    else
      sections.push({
        id: `feature-${slug(label)}`,
        label,
        description,
        kind: kind(id, label),
        capability: true,
      });
  }
  for (const [section, label, layout] of [
    ["overview", "Overview", "analytics"],
    ["records", "All records", "workflow"],
    ["activity", "Activity", "timeline"],
  ]) {
    if (!sections.some((item) => item.id === section))
      sections.push({
        id: section,
        label,
        description: spec.focus,
        kind: layout,
        capability: false,
      });
  }
  return {
    id,
    name: seed.CTX[id].n,
    group: seed.GROUPS.find((group) => group[1].includes(id))[0],
    icon: seed.LICON[id],
    description: spec.focus,
    recordType: spec.kind,
    action: spec.create,
    primary: sections.slice(0, 3).map((section) => section.id),
    sections,
  };
});
if (
  registry.length !== 31 ||
  registry.flatMap((item) => item.sections).filter((item) => item.capability)
    .length !== 171
)
  throw new Error("Module coverage changed; review the manifest.");
fs.writeFileSync(
  "src/data/module-registry.json",
  JSON.stringify(registry, null, 2) + "\n",
);
console.log("Generated the 31-module IAM and backend registry.");
