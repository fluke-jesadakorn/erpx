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
];

export function getSection(key: string): Section | undefined {
  return SECTION_CATALOG.find(s => s.key === key);
}