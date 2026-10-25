export type InvestorGroup = "erp" | "hospitality" | "collaboration";
export type InvestorCheckpoint = { id: string; title: string; role: string; caption: string; mobile?: true };
export type InvestorJourney = { id: string; group: InvestorGroup; suite: string; file: string; title: string; checkpoints: InvestorCheckpoint[] };
export const investorGroups = [
  { id: "erp", title: "Connected ERP", narrative: "Carry commercial intent through delivery, inventory and independently reviewed finance. Every handoff retains its source record." },
  { id: "hospitality", title: "Hotel & POS", narrative: "Connect guest service and restaurant operations with guest accounts, cash control and bank reconciliation." },
  { id: "collaboration", title: "AI & Collaboration", narrative: "Keep accountable actions, team conversations and contextual assistance close to the work they support." },
] as const;
export const investorJourneys: InvestorJourney[] = [
  { id: "sales", group: "erp", suite: "sales.demo-sales-to-collection.journeys", file: "tests/e2e/demo-sales-to-collection.spec.ts", title: "isolated demo carries a quotation through sales delivery, customer invoice and collection", checkpoints: [
    { id: "sales-quote", title: "Quotation ready for conversion", role: "Sales operator", caption: "An independently approved quotation becomes the source of the sales workflow." },
    { id: "sales-invoice", title: "Delivery becomes a posted receivable", role: "Finance approver", caption: "Delivery evidence flows into an invoice reviewed and posted by Finance." },
    { id: "sales-paid", title: "Customer collection completed", role: "Treasury", caption: "The invoice retains its payment history and has zero outstanding after collection.", mobile: true },
  ] },
  { id: "purchase", group: "erp", suite: "procurement.demo-purchase-request-rfq.e2e", file: "tests/e2e/demo-purchase-request-rfq.spec.ts", title: "isolated demo carries an approved purchase request through split RFQs, receipts, one consolidated AP invoice and supplier payment", checkpoints: [
    { id: "purchase-approved", title: "Approved demand enters sourcing", role: "Purchasing operator", caption: "Purchasing allocates an approved employee request into supplier quotation requests." },
    { id: "purchase-receipts", title: "Two receipts, one supplier invoice", role: "Finance preparer", caption: "Six-unit and four-unit receipts retain their quantities in a consolidated supplier invoice review." },
    { id: "purchase-paid", title: "Supplier payment confirmed", role: "Treasury", caption: "Independent approval and matching bank evidence settle the consolidated payable." },
  ] },
  { id: "finance", group: "erp", suite: "finance.demo-finance-reports.e2e", file: "tests/e2e/demo-finance-reports.spec.ts", title: "isolated demo traces finance report totals to the selected ledger", checkpoints: [
    { id: "finance-statements", title: "Financial statements with source links", role: "Finance approver", caption: "Statements use the selected ledger and expose links for tracing reported totals.", mobile: true },
    { id: "finance-sources", title: "Trace a report to journal entries", role: "Finance approver", caption: "A record drawer connects a reported asset total to its account and underlying journals." },
  ] },
  { id: "hotel", group: "hospitality", suite: "pms.thai-hotel-v1-hotel.journeys", file: "tests/e2e/thai-hotel-v1-hotel.spec.ts", title: "H002 guest journey includes independent guest account correction, payment, checkout and Night Audit", checkpoints: [
    { id: "hotel-checkin", title: "Guest checked in to an inspected room", role: "Front desk", caption: "Room readiness, assignment and check-in converge in the front-desk workspace.", mobile: true },
    { id: "hotel-guest-account", title: "Corrected guest account with zero balance", role: "Hotel cashier", caption: "Independent charge correction and payment preserve the guest account trail before checkout." },
    { id: "hotel-audit", title: "Night Audit ready for day close", role: "Night auditor", caption: "Reviewed shifts and resolved guest account corrections allow the hotel day to close." },
  ] },
  { id: "pos-cash", group: "hospitality", suite: "pos.thai-hotel-v1-pos.journeys", file: "tests/e2e/thai-hotel-v1-pos.spec.ts", title: "H002 restaurant posts a cash sale, records waste, reviews a refund and closes its cash session", checkpoints: [
    { id: "pos-sale", title: "Restaurant sale and cash tender", role: "POS cashier", caption: "The selling workspace brings the basket, tender and sale completion together.", mobile: true },
    { id: "pos-refund", title: "Independent refund review", role: "POS manager", caption: "A source-linked return reaches a separate manager with its evidence and approval action." },
    { id: "pos-shift", title: "Cash shift closes without variance", role: "POS cashier", caption: "Counted cash agrees with expected cash after sale and refund activity." },
  ] },
  { id: "pos-transfer", group: "hospitality", suite: "pos.thai-hotel-v1-pos.journeys", file: "tests/e2e/thai-hotel-v1-pos.spec.ts", title: "H002 restaurant transfer stays unsettled until independent Finance matches its imported bank row", checkpoints: [
    { id: "pos-pending", title: "Transfer awaits independent verification", role: "POS cashier", caption: "The interface prevents completing a sale while its bank transfer remains unverified." },
    { id: "pos-verified", title: "Matched transfer unlocks completion", role: "POS cashier", caption: "Finance matches the imported bank row; the cashier can then complete the sale." },
  ] },
  { id: "action", group: "collaboration", suite: "collaboration.demo-collaboration-action-card.e2e", file: "tests/e2e/demo-collaboration-action-card.spec.ts", title: "isolated demo routes a record-linked action card to its owner", checkpoints: [
    { id: "action-created", title: "An action linked to its sales source", role: "Holding owner", caption: "An accountable next action retains its source document, owner and due date." },
    { id: "action-owner", title: "The assigned owner receives the action", role: "Collaboration owner", caption: "The recipient works from the same source-linked action drawer.", mobile: true },
    { id: "action-completed", title: "Completed action retains evidence", role: "Collaboration owner", caption: "Completion and supporting evidence remain available after reload." },
  ] },
  { id: "social", group: "collaboration", suite: "collaboration.demo-collaboration-social.e2e", file: "tests/e2e/demo-collaboration-social.spec.ts", title: "isolated demo publishes a mentioned update and keeps thread actions after reload", checkpoints: [
    { id: "social-update", title: "Team update with a contextual mention", role: "Holding owner", caption: "A published operations handoff mentions an agent within the shared workspace." },
    { id: "social-thread", title: "Acknowledgement survives reload", role: "Employee", caption: "Comments, appreciation and following state remain attached to the conversation." },
  ] },
  { id: "ai", group: "collaboration", suite: "pms.thai-hotel-v1-ai.journeys", file: "tests/e2e/thai-hotel-v1-ai.spec.ts", title: "H002 AI drafts from authorized versions, cites hotel evidence, then falls back manually", checkpoints: [
    { id: "ai-draft", title: "Source-aware draft preparation", role: "Employee", caption: "A suggested title identifies the saved request version and remains subject to human review." },
    { id: "ai-assistant", title: "Assistance within the current workspace", role: "Night auditor", caption: "The work assistant responds in the context of the current page. This E2E uses a deterministic AI adapter.", mobile: true },
    { id: "ai-manual", title: "Work continues when AI is unavailable", role: "Employee", caption: "A clear availability message leaves the ordinary draft workflow usable." },
  ] },
];
export const investorCases = investorJourneys.flatMap(journey => [
  { suite: journey.suite, title: journey.title, project: "desktop" as const },
  ...(journey.checkpoints.some(point => point.mobile) ? [{ suite: journey.suite, title: journey.title, project: "mobile" as const }] : []),
]);
export const investorShots = investorJourneys.flatMap(journey => journey.checkpoints.flatMap(point => [
  { journey: journey.id, group: journey.group, ...point, project: "desktop" as const },
  ...(point.mobile ? [{ journey: journey.id, group: journey.group, ...point, project: "mobile" as const }] : []),
]));
