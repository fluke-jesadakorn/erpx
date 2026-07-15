export interface QuickPrompt {
  icon: string;
  label: string;
  label_th?: string;
  label_de?: string;
  prompt: string;
  extract?: boolean;
}

export const QUICK_PROMPTS: Record<string, QuickPrompt[]> = {
  cockpit: [
    { icon: '💵', label: 'Cash · 6mo',         label_th: 'เงินสด · 6 เดือน',         label_de: 'Liquidität · 6 Mo.',          prompt: 'Show me the cash balance over the last 6 months as a chart.' },
    { icon: '🏷️', label: 'Top 5 expense categories', label_th: '5 หมวดค่าใช้จ่ายสูงสุด', label_de: 'Top 5 Ausgabenkategorien', prompt: 'Chart the top 5 expense categories by total amount this month.' },
    { icon: '⏳', label: 'Stuck items (24h)',  label_th: 'รายการที่ค้าง (24 ชม.)',  label_de: 'Hängende Posten (24h)',     prompt: 'List any waybills that have been stuck in the same stage for more than 24 hours.' },
    { icon: '🔥', label: 'Burn rate · this month', label_th: 'อัตราเผาผลาญ · เดือนนี้', label_de: 'Burn-Rate · diesen Monat', prompt: 'Plot the daily burn rate for this month as a chart.' },
    { icon: '🚨', label: 'Anomalies this week', label_th: 'ความผิดปกติสัปดาห์นี้', label_de: 'Anomalien diese Woche',     prompt: 'Highlight any anomalies detected this week in the pipeline.' },
  ],
  expense: [
    { icon: '📅', label: "Today's expenses",   label_th: 'ค่าใช้จ่ายวันนี้',         label_de: 'Heutige Ausgaben',           prompt: 'Summarize my expenses submitted today.' },
    { icon: '📝', label: 'Draft summary',      label_th: 'สรุปแบบร่าง',             label_de: 'Entwurfs-Zusammenfassung',  prompt: 'Show the current state of my active expense draft.' },
    { icon: '🔁', label: 'Duplicate check',    label_th: 'ตรวจรายการซ้ำ',          label_de: 'Duplikate prüfen',          prompt: 'Check if any of my recent expenses look like duplicates.' },
    { icon: '🧾', label: 'Paste a receipt to extract', label_th: 'วางใบเสร็จเพื่อดึงข้อมูล', label_de: 'Beleg einfügen zum Extrahieren', prompt: 'I just paid a vendor. Here is the receipt description:', extract: true },
  ],
  waybill: [
    { icon: '📜', label: 'Audit trail summary', label_th: 'สรุป Audit Trail',         label_de: 'Audit-Trail-Zusammenfassung', prompt: 'Summarize the audit trail for this waybill.' },
    { icon: '📍', label: 'Current stage',      label_th: 'ขั้นตอนปัจจุบัน',          label_de: 'Aktuelle Stufe',            prompt: 'What stage is this waybill at right now and what does it mean?' },
    { icon: '❌', label: 'Explain last rejection', label_th: 'อธิบายการปฏิเสธล่าสุด', label_de: 'Letzte Ablehnung erklären', prompt: 'Explain the most recent rejection on this waybill and what to do next.' },
  ],
  ledger: [
    { icon: '📊', label: 'Top variance lines', label_th: 'รายการ GL ที่มี Variance สูงสุด', label_de: 'Top-Abweichungszeilen',   prompt: 'Show the top GL lines by variance this period as a chart.' },
    { icon: '📈', label: 'Month over month',   label_th: 'เปรียบเทียบเดือนต่อเดือน', label_de: 'Monat zu Monat',          prompt: 'Compare this month vs last month totals per category in a chart.' },
    { icon: '💳', label: 'Biggest debit · 30d', label_th: 'เดบิตสูงสุด · 30 วัน',     label_de: 'Größte Belastung · 30 T.', prompt: 'What was the single biggest debit line in the last 30 days?' },
  ],
  pr: [
    { icon: '📋', label: 'Open PRs',           label_th: 'PR ที่เปิดอยู่',           label_de: 'Offene PRs',                prompt: 'How many PRs are open and what is their total value?' },
    { icon: '⏱️', label: 'Slowest PRs',         label_th: 'PR ที่ใช้เวลาอนุมัตินานที่สุด', label_de: 'Langsamste PRs',     prompt: 'Which PRs are taking the longest to approve?' },
    { icon: '💼', label: 'Top requesters',     label_th: 'ผู้ขอ PR สูงสุด',         label_de: 'Top-Antragsteller',         prompt: 'Who requested the most PRs this quarter? Chart it.' },
  ],
  po: [
    { icon: '📦', label: 'Open POs',           label_th: 'PO ที่เปิดอยู่',           label_de: 'Offene POs',                prompt: 'Summarize all open purchase orders and their remaining balances.' },
    { icon: '🏪', label: 'Top vendors',        label_th: 'ผู้ขายสูงสุด',             label_de: 'Top-Lieferanten',           prompt: 'Which vendors have the highest PO total this quarter? Chart it.' },
    { icon: '⚠️', label: 'Over-budget POs',    label_th: 'PO ที่เกินงบ',             label_de: 'POs über Budget',           prompt: 'List any POs that are over their original budget.' },
  ],
  policy: [
    { icon: '📖', label: 'Active policies',    label_th: 'นโยบายที่ใช้งานอยู่',       label_de: 'Aktive Richtlinien',         prompt: 'What approval policies are currently active?' },
    { icon: '🧩', label: 'Stage explainer',    label_th: 'อธิบายขั้นตอนอนุมัติ',       label_de: 'Stufen-Erklärung',           prompt: 'Explain the approval stages in plain language.' },
    { icon: '🛠️', label: 'Edit a policy',      label_th: 'แก้ไขนโยบาย',             label_de: 'Richtlinie bearbeiten',      prompt: 'Walk me through how to edit a policy without breaking existing waybills.' },
  ],
  sales: [
    { icon: '📋', label: 'Open orders',        label_th: 'ออเดอร์ที่เปิดอยู่',       label_de: 'Offene Aufträge',           prompt: 'How many sales orders are open and what is their total value?' },
    { icon: '🏪', label: 'Top customers',      label_th: 'ลูกค้าสูงสุด',             label_de: 'Top-Kunden',                prompt: 'Which customers have the highest total invoiced this quarter? Chart it.' },
    { icon: '⚠️', label: 'Overdue AR',         label_th: 'AR ที่เกินกำหนด',           label_de: 'Überfällige AR',            prompt: 'List any sales orders past their due_date that are still unpaid.' },
    { icon: '💰', label: 'MTD revenue',        label_th: 'รายได้ MTD',               label_de: 'Umsatz MTD',                prompt: 'How much revenue did we book this month? Chart it weekly.' },
    { icon: '📈', label: 'Forecast next month', label_th: 'คาดการณ์เดือนหน้า',         label_de: 'Prognose nächsten Monat',   prompt: "Forecast next month's revenue based on the last 90 days trend." },
  ],
  customers: [
    { icon: '🏢', label: 'All active customers', label_th: 'ลูกค้าที่ใช้งานอยู่ทั้งหมด', label_de: 'Alle aktiven Kunden',   prompt: 'List all active customers with their outstanding AR.' },
    { icon: '🔍', label: 'Credit limit breaches', label_th: 'เกินวงเงินเครดิต',         label_de: 'Kreditlimit-Überschreitungen', prompt: 'Which customers are above 80% of their credit limit?' },
    { icon: '🚫', label: 'Blacklist',          label_th: 'บัญชีดำ',                 label_de: 'Sperrliste',                prompt: 'Show me the blacklisted customers and their outstanding balance.' },
  ],
};