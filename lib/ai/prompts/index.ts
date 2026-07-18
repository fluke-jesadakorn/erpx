import type { SqlSummaryTemplates } from './en';
import { en } from './en';
import { th } from './th';
import { de } from './de';

export type { SqlSummaryTemplates };
export { en } from './en';

export type SqlLang = 'en' | 'th' | 'de';

const BY_LANG: Record<SqlLang, SqlSummaryTemplates> = { en, th, de };

export function templatesFor(lang: SqlLang): SqlSummaryTemplates {
  return BY_LANG[lang] ?? en;
}