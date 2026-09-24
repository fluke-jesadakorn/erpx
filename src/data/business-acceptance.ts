import registry from "./module-registry.json";
import { capabilityDelivery } from "./capability-delivery";

export const acceptanceGates = ["automated", "browser", "provider", "production"] as const;
export type AcceptanceGate = typeof acceptanceGates[number];
export type GateEvidence = {
  state: "pending" | "passed" | "not_applicable";
  evidence: string | null;
  reviewer: string | null;
  reviewedAt: string | null;
};
type Ownership = { phase: number; owner: string; scenarios: number[]; gaps: number[] };
const ownership: Record<string, Ownership> = {
  brand: { phase: 8, owner: "Brand / Company administration", scenarios: [19], gaps: [17] },
  commercial: { phase: 8, owner: "Platform Finance", scenarios: [15], gaps: [14] },
  identity_access: { phase: 0, owner: "Company administration / IT", scenarios: [12, 13, 15], gaps: [11, 20] },
  organization: { phase: 7, owner: "HR / Company administration", scenarios: [12, 13], gaps: [11] },
  platform_administration: { phase: 8, owner: "Platform operations", scenarios: [15, 19], gaps: [14, 17, 19] },
  manage_erpx: { phase: 8, owner: "Platform operations / Platform Finance", scenarios: [15, 19], gaps: [14, 17, 19] },
  customer_360: { phase: 3, owner: "Sales / Customer service", scenarios: [1, 20], gaps: [1] },
  customer_authority: { phase: 3, owner: "Customer service / Data owner", scenarios: [1, 20], gaps: [1] },
  support: { phase: 3, owner: "Customer service", scenarios: [4, 10, 15], gaps: [1, 9] },
  finance: { phase: 1, owner: "Financial controller / Treasury", scenarios: [2, 4, 14], gaps: [4, 12] },
  pos: { phase: 5, owner: "Store manager / Finance", scenarios: [5, 6, 8], gaps: [6, 8, 13] },
  sales: { phase: 1, owner: "Sales / Logistics", scenarios: [1, 2, 4], gaps: [2, 4] },
  assets: { phase: 4, owner: "Asset controller", scenarios: [12, 13], gaps: [11] },
  inventory: { phase: 2, owner: "Warehouse controller", scenarios: [2, 3, 14], gaps: [3, 12] },
  maintenance: { phase: 4, owner: "Maintenance", scenarios: [7, 10], gaps: [9] },
  operations: { phase: 4, owner: "Operations", scenarios: [9, 10, 12], gaps: [2, 5, 9] },
  procurement: { phase: 2, owner: "Purchasing", scenarios: [3, 12], gaps: [3] },
  quality: { phase: 2, owner: "Quality", scenarios: [3, 4, 11], gaps: [3, 4, 10] },
  legal: { phase: 4, owner: "Legal / Company policy owner", scenarios: [1, 9, 21], gaps: [5, 17] },
  people_payroll: { phase: 7, owner: "HR / Payroll / Finance", scenarios: [12, 13], gaps: [11] },
  crm: { phase: 3, owner: "Sales", scenarios: [1, 20], gaps: [1] },
  marketing: { phase: 3, owner: "Marketing", scenarios: [1, 16], gaps: [1, 15] },
  manufacturing: { phase: 6, owner: "Production / Cost accounting", scenarios: [11], gaps: [10] },
  planning: { phase: 6, owner: "Planning", scenarios: [3, 11], gaps: [3, 10] },
  projects: { phase: 4, owner: "Project management", scenarios: [9], gaps: [5] },
  ecommerce: { phase: 5, owner: "E-commerce / Sales", scenarios: [16], gaps: [15] },
  benefits: { phase: 7, owner: "HR benefits", scenarios: [18], gaps: [16] },
  learning: { phase: 7, owner: "HR learning", scenarios: [18], gaps: [16] },
  talent: { phase: 7, owner: "Recruitment", scenarios: [17], gaps: [16] },
  field_service: { phase: 4, owner: "Field service", scenarios: [10], gaps: [9] },
  agent: { phase: 8, owner: "AI governance / Source data owners", scenarios: [20], gaps: [18] },
  collaboration: { phase: 8, owner: "Workplace / Source data owners", scenarios: [12, 20], gaps: [18] },
  pms: { phase: 5, owner: "Hotel manager / Finance", scenarios: [7, 8], gaps: [7, 8] },
};
const id = (prefix: string, value: number) => `${prefix}${String(value).padStart(2, "0")}`;

/** Gates are independent attestations, never inferred from a module-wide label. */
export function businessAcceptanceManifest() {
  return registry.flatMap(module => module.sections.filter(s => s.capability).map(section => {
    const responsibility = ownership[module.id];
    const delivery = capabilityDelivery(module.id, section.id);
    if (!responsibility || !delivery) throw new Error(`Missing acceptance mapping: ${module.id}/${section.id}`);
    const gates = Object.fromEntries(acceptanceGates.map(gate => [gate, {
      state: "pending", evidence: null, reviewer: null, reviewedAt: null,
    }])) as Record<AcceptanceGate, GateEvidence>;
    return {
      id: `${module.id}/${section.id}`, label: section.label,
      phase: responsibility.phase, owner: responsibility.owner,
      scenarios: responsibility.scenarios.map(value => id("S", value)),
      gaps: responsibility.gaps.map(value => id("G", value)),
      implementation: delivery,
      gates,
    };
  }));
}
