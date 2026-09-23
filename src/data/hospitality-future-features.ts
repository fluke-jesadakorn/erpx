export type FutureFeature = {
  id: string;
  name: string;
  value: string;
  stage: "Coming Soon" | "Prototype";
  prerequisite: string;
};

/** Product catalogue, not a statement of production acceptance. */
export const hospitalityFutureFeatures: FutureFeature[] = [
  { id: "b2b-sales", name: "Broad B2B sales", value: "Quotes, customer orders and collections beyond hotel and restaurant transactions.", stage: "Coming Soon", prerequisite: "Hotel pilot reconciliation and separate customer-order acceptance" },
  { id: "direct-booking", name: "Direct online booking", value: "Guests reserve rooms without front-desk entry.", stage: "Coming Soon", prerequisite: "Pilot reservation acceptance, real email delivery and customer-facing booking review" },
  { id: "ota", name: "OTA synchronization", value: "Share availability and reservations with a named channel.", stage: "Coming Soon", prerequisite: "Direct booking acceptance, provider contract and conflict/recovery tests" },
  { id: "card", name: "Live card and PromptPay", value: "Accept online or terminal payments.", stage: "Coming Soon", prerequisite: "Provider approval, refunds, settlement reconciliation and interrupted-payment recovery" },
  { id: "mixed-tender", name: "Mixed room and cash tender", value: "Split a restaurant bill between a room folio and another payment.", stage: "Coming Soon", prerequisite: "Full-bill room-charge reversals and reconciliation accepted" },
  { id: "offline-pos", name: "Offline restaurant selling", value: "Keep serving during an internet outage.", stage: "Coming Soon", prerequisite: "Device identity, conflict resolution and duplicate-prevention acceptance" },
  { id: "loyalty", name: "Guest loyalty", value: "Recognize returning guests and governed offers.", stage: "Prototype", prerequisite: "Consent, customer authority and retention design" },
  { id: "multi-property", name: "Multiple properties", value: "Operate hotels from one controlled company workspace.", stage: "Coming Soon", prerequisite: "One-property pilot close, property isolation and cross-property reporting" },
  { id: "native-etax", name: "Native Thai e-Tax", value: "Issue legal electronic tax documents from ERPX.", stage: "Coming Soon", prerequisite: "Accountant and provider acceptance; certified document, signature and submission process" },
  { id: "payroll", name: "Statutory payroll", value: "Run country-specific payroll and filings.", stage: "Coming Soon", prerequisite: "Thailand statutory validation and independent payroll sign-off" },
  { id: "ecommerce", name: "E-commerce storefront", value: "Sell products online with shared inventory.", stage: "Prototype", prerequisite: "Hospitality stock and payment controls accepted" },
  { id: "autonomous-ai", name: "Autonomous AI actions", value: "Coordinate routine work under explicit authority.", stage: "Prototype", prerequisite: "Scoped evidence adapters, evaluation, monitoring and human approval controls" },
];
