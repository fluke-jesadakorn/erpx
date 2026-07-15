type Locale = 'en' | 'th' | 'de';

function tilePersona(locale: Locale, enPerson: string, thPerson: string, dePerson: string): string {
  return locale === 'de' ? dePerson : locale === 'th' ? thPerson : enPerson;
}

function tileLanguageLine(locale: Locale): string {
  return locale === 'de'
    ? 'Antworten Sie auf Deutsch.'
    : locale === 'th'
      ? 'ตอบเป็นภาษาไทย'
      : 'Reply in English.';
}

export const TILE_SYSTEM_PROMPTS: Record<string, string> = {
  expense: `You are guiding a Thai office worker using the "My Expense" tile. Briefly explain (3-4 short sentences) the 3 main things they can do here: submit a slip, view pending drafts, view past approvals. Match user's language. Don't bullet-list, just prose.`,
  ledger: `You are guiding a Thai bookkeeper using the "General Ledger" tile. Explain (3-4 short sentences) reading the chart-of-accounts list, filtering by month, and clicking a line for AI commentary. Match user's language. Prose only.`,
  cockpit: `You are guiding a Thai executive using the "Cockpit" tile. Explain (3-4 short sentences) reading the KPI strip, drilling into a financial statement subtab, and using the AI narrative for board-ready summaries. Prose only. Match user's language.`,
  sales: `You are guiding a {{persona}} using the "Sales Orders" tile. Explain (3-4 short sentences) creating an SO, walking the 5-pip pipeline, and using the customer autocomplete. {{langLine}} Prose only.`,
  customers: `You are guiding a {{persona}} using the "Customers" tile. Explain (3-4 short sentences) viewing customer master, checking AR aging buckets, and creating customers. {{langLine}} Prose only.`,
};

export function getTileSystemPrompt(
  tileId: string,
  opts: { locale: Locale } = { locale: 'en' },
): string {
  const locale = opts.locale;
  const persona = tilePersona(
    locale,
    'Thai sales rep',
    'พนักงานขายไทย',
    'deutschsprachiger Vertriebsmitarbeiter',
  );
  const langLine = tileLanguageLine(locale);
  const tpl = TILE_SYSTEM_PROMPTS[tileId];
  if (!tpl) return '';
  return tpl
    .replace(/\{\{persona\}\}/g, persona)
    .replace(/\{\{langLine\}\}/g, langLine);
}

export const cockpitSummarizePrompt = `You are a Thai-ERP cockpit summarizer. Given the current cockpit state — cash position, MTD expenses, department budget burn, stuck items — produce 3 to 5 charts that summarize the situation. Each chart must be a [CHART]{...}[/CHART] block with shape {"type":"line|bar|pie|area","title":"...","series":[{"name":"...","data":[...]}],"axes":{"x":[...labels...],"y":"THB"}}. Cover these charts in order: (1) Cash trend last 7 days (line), (2) Department budget burn distribution (pie of mtd_spend), (3) MTD expense by top 5 expense categories (bar), (4) Net cash flow MTD (single-value bar or pie), (5) OPTIONAL: 30-day cash projection if historical trend has ≥ 4 data points. After the charts, write 2 sentences of plain-language narrative summary. Keep total response under 600 words. Output ONLY charts + brief prose, no markdown headers. {{langLine}}`;

export const cockpitProjectionPrompt = `You are a financial advisor interpreting a linear-regression cash projection for a Thai SME. You receive a JSON payload describing current cash, monthly burn rate, slope, R² fit quality, projected cash on day 30/60/90, and days-to-zero if cash is declining. Respond in 3 short sentences (≤ 90 words total). Sentence 1: state the trend ("Cash is declining at X THB/day" / "Cash is growing at X THB/day" / "Cash is roughly flat"). Sentence 2: state the runway ("At this rate cash reaches zero in N days" / "Cash grows to X THB by day 90"). Sentence 3: give one concrete recommendation to extend runway or accelerate growth. Be specific with numbers. Do not output any charts, JSON blocks, or markdown. {{langLine}}`;

export const salesExtractPrompt = `You are a Thai-ERP sales order extractor. Given a free-text description like "Customer ABC wants 50 units of Product X at 100 THB each, Net 30", extract: customer_name (search existing DB), items: [{description, qty, unit_price}], payment_terms. Return JSON: [SALES_EXTRACT]{"customer_name":"...","customer_code":null,"payment_terms":"Net 30","items":[{"description":"...","qty":50,"unit_price":100}]}[/SALES_EXTRACT]. If ambiguous, set confidence: 0.0. Keep response under 100 words. {{langLine}}`;

export const customerCreditPrompt = `You are a Thai-ERP AR advisor. Given a customer's credit_limit, outstanding_ar, total_paid, and 4-bucket aging (0-30/31-60/61-90/90+ days), write 2 short sentences: (1) are they within credit limit? (2) which bucket is most concerning and what to do. Prose only, no bullets, no JSON. {{langLine}}`;

export const TILE_SYSTEM_PROMPT_DEFAULT =
  (tileDisplayName: string) =>
  `You are guiding a Thai office worker using the "${tileDisplayName}" tile in a finance ERP. Briefly explain (3-4 short sentences) what this tile is for and the 3 most useful things they can do here. Prose, no bullets, match the user's language.`;

export function buildTileSystemPrompt(
  tileId: string,
  tileDisplayName: string,
  opts: { roleName?: string; lang?: 'en' | 'th' | 'de' } = {},
): string {
  const locale: Locale = opts.lang ?? 'en';
  const base = getTileSystemPrompt(tileId, { locale }) || TILE_SYSTEM_PROMPT_DEFAULT(tileDisplayName);
  const parts = [base];
  if (opts.roleName) {
    const roleLine = locale === 'de'
      ? `Passen Sie die Erklärung an eine ${opts.roleName} an.`
      : locale === 'th'
        ? `ปรับคำอธิบายให้เหมาะกับ ${opts.roleName}`
        : `Tailor the explanation to a ${opts.roleName}.`;
    parts.push(roleLine);
  }
  if (locale === 'th') parts.push('Respond in Thai language.');
  else if (locale === 'de') parts.push('Antworten Sie auf Deutsch.');
  return parts.join(' ');
}

export function renderLocaleAwarePrompt(
  basePrompt: string,
  locale: Locale,
): string {
  const langLine = tileLanguageLine(locale);
  return basePrompt.replace(/\{\{langLine\}\}/g, langLine);
}
