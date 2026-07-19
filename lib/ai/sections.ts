// AI Settings — section catalog
// Each section maps to a (task_type) that the router resolves to a provider/model

export type AITask = 'embed' | 'chat' | 'vision';

export interface Section {
  key: string;          // matches tile id or sub-feature, e.g. 'acct:coa-search'
  label: string;
  labelTh: string;
  task: AITask;
  description?: string;
}

export const SECTION_CATALOG: Section[] = [
  ...(['global', 'hub', 'expense', 'sales', 'customers', 'pr', 'po', 'cockpit', 'executive', 'ledger', 'policy', 'tiles', 'audit', 'ai-settings', 'hr', 'law', 'org', 'inbox', 'waybill'] as const).map((zone) => ({
    key: `chat:${zone}`,
    label: `${zone} contextual chat`,
    labelTh: `${zone} contextual chat`,
    task: 'chat' as const,
    description: `Contextual AI chat for the ${zone} zone.`,
  })),
  {
    key: 'hr:classify-intent', label: 'HR intent classifier', labelTh: 'จำแนกคำขอ HR', task: 'chat',
  },
  {
    key: 'customer:credit-check', label: 'Customer credit check', labelTh: 'ตรวจเครดิตลูกค้า', task: 'chat',
  },
  {
    key: 'am:recommend', label: 'Approval recommendation', labelTh: 'แนะนำการอนุมัติ', task: 'chat',
  },
  {
    key: 'manager:approve', label: 'Manager approval helper', labelTh: 'ผู้ช่วยอนุมัติผู้จัดการ', task: 'chat',
  },
  {
    key: 'waybill:assist', label: 'Waybill assistant', labelTh: 'ผู้ช่วยเวย์บิล', task: 'chat',
  },
  {
    key: 'waybill:settle', label: 'Waybill settlement vision', labelTh: 'ตรวจเอกสารชำระเวย์บิล', task: 'vision',
  },
  {
    key: 'staff-test:accountant-reviewer', label: 'Accountant reviewer test', labelTh: 'ทดสอบผู้ช่วยบัญชี', task: 'chat',
  },
  {
    key: 'staff:ocr',
    label: 'Receipt OCR',
    labelTh: 'Receipt Scan (OCR)',
    task: 'vision',
    description: 'Vision model that extracts line items, amount, merchant from a receipt photo.',
  },
  {
    key: 'staff:submit',
    label: 'Expense submission helper',
    labelTh: 'Expense Form Helper',
    task: 'chat',
    description: 'Chat assistant that suggests descriptions and categories as staff fill the form.',
  },
  {
    key: 'acct:coa-search',
    label: 'COA semantic mapping',
    labelTh: 'Map COA Codes with Semantic',
    task: 'embed',
    description: 'Embedding model used to rank chart-of-account candidates by similarity.',
  },
  {
    key: 'acct:queue',
    label: 'Expense anomaly detection',
    labelTh: 'Detect Anomalous Items',
    task: 'chat',
    description: 'Chat model that flags suspicious amounts, duplicates, or out-of-policy items.',
  },
  {
    key: 'hod:approve',
    label: 'Approval comment summarizer',
    labelTh: 'Summarize Approver Comments',
    task: 'chat',
    description: 'Summarizes the approval thread for the head of department.',
  },
  {
    key: 'am:review',
    label: 'Policy recommendation',
    labelTh: 'Recommend Policy',
    task: 'chat',
    description: 'Recommends which approval policy matches an expense context.',
  },
  {
    key: 'cfo:cockpit',
    label: 'Executive narrative',
    labelTh: 'Executive Summary Narrative',
    task: 'chat',
    description: 'Generates a narrative paragraph for the CFO cockpit tile.',
  },
  {
    key: 'ceo:cockpit',
    label: 'Board summary',
    labelTh: 'Board Summary',
    task: 'chat',
    description: 'High-level board-ready summary for CEO cockpit.',
  },
  {
    key: 'ledger:commentary',
    label: 'GL commentary',
    labelTh: 'GL Line Commentary',
    task: 'chat',
    description: 'Explains ledger line variances in plain language.',
  },
  {
    key: 'policy:editor',
    label: 'Policy linting',
    labelTh: 'Review Policy',
    task: 'chat',
    description: 'Reviews a draft approval policy for contradictions and gaps.',
  },
  {
    key: 'command:intent',
    label: 'Command palette intent',
    labelTh: 'Predict Command ⌘K',
    task: 'chat',
    description: 'Interprets natural-language command palette input and picks a destination.',
  },
  {
    key: 'notification:digest',
    label: 'Notification digest',
    labelTh: 'Notification Digest',
    task: 'chat',
    description: 'Condenses a batch of domain events into a single human-readable digest.',
  },
  {
    key: 'sales:extract',
    label: 'Sales Order extractor',
    labelTh: 'แยกข้อความเป็นใบสั่งขาย',
    task: 'chat',
    description: 'Extracts customer + line items from a free-text description into a draft SO.',
  },
  {
    key: 'customer:advisory',
    label: 'Customer credit advisory',
    labelTh: 'คำแนะนำเครดิตลูกค้า',
    task: 'chat',
    description: 'Two-sentence advisory based on credit limit and AR aging.',
  },
  {
    key: 'cockpit:sql',
    label: 'Chat-to-SQL',
    labelTh: 'ถามข้อมูลด้วย SQL',
    task: 'chat',
    description: 'Translates natural-language questions into read-only SQL queries against allow-listed tables.',
  },
  {
    key: 'cockpit:projection',
    label: 'Cash projection interpreter',
    labelTh: 'ตีความการคาดการณ์เงินสด',
    task: 'chat',
    description: 'Interprets the 90-day cash projection summary in 2-3 sentences of executive prose.',
  },
  {
    key: 'cockpit:summarize',
    label: 'Cockpit daily brief summarizer',
    labelTh: 'สรุปข้อมูลค็อกพิทรายวัน',
    task: 'chat',
    description: 'Condenses today\'s cockpit metrics (cash, MTD burn, approvals) into a 2-3 sentence exec brief.',
  },
  {
    key: 'finance:rag',
    label: 'Finance RAG answer',
    labelTh: 'คำตอบจากข้อมูลการเงิน',
    task: 'chat',
    description: 'Answers questions about historical expenses, vendors, and patterns using RAG over folio.vendor_embeddings.',
  },
  {
    key: 'hr:agent',
    label: 'HR LINE agent',
    labelTh: 'บอท HR (LINE)',
    task: 'chat',
    description: 'Tool-calling HR agent for LINE — handles leave requests, balance checks, team schedules.',
  },
  {
    key: 'chat:full',
    label: 'Full AI Chat',
    labelTh: 'แชท AI แบบเต็ม',
    task: 'chat',
    description: 'General-purpose AI assistant. Renders charts, HTML reports, and runs read-only SQL via [SQL] blocks.',
  },
  {
    key: 'events:explain',
    label: 'Waybill event explainer',
    labelTh: 'อธิบายเหตุการณ์เวย์บิล',
    task: 'chat',
    description: 'Explains a single waybill event (kind + from/to stage) in plain language for the actor.',
  },
  {
    key: 'law:rag',
    label: 'Law RAG answer',
    labelTh: 'ตอบคำถามกฎหมาย (RAG)',
    task: 'chat',
    description: 'Answers law-document questions using only the retrieved chunk context.',
  },
  {
    key: 'law:contracts',
    label: 'Law contract embedder',
    labelTh: 'ฝังข้อความสัญญา',
    task: 'embed',
    description: 'Embedding model used to vectorize contract chunks for similarity search.',
  },
];

export type SectionKey = (typeof SECTION_CATALOG)[number]['key'];

export function getSection(key: SectionKey): Section;
export function getSection(key: string): Section | undefined;
export function getSection(key: string): Section | undefined {
  return SECTION_CATALOG.find(s => s.key === key);
}

export function assertSection(k: string): asserts k is SectionKey {
  if (!getSection(k)) {
    throw new Error(`Unknown AI section key: ${k}`);
  }
}
