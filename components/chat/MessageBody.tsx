import { ChartRenderer } from './ChartRenderer';
import { HtmlRenderer } from './HtmlRenderer';
import { SqlResultTable } from './SqlResultTable';
import { SafeUiRenderer } from './SafeUiRenderer';
import { Panel } from '@/components/ui';
import type { ChartSpec } from '@/components/chat/chartContract';
import type { SqlResolved } from '@folio-lib/chat/history';
import type { UiRoot } from '@folio-lib/ai/safeUiContract';

export interface MessageBodyInput {
  content: string;
  charts?: ChartSpec[];
  htmls?: string[];
  sqls?: SqlResolved[];
  uis?: Array<{ id?: string; root?: UiRoot | null }>;
}

export function MessageBody({ content, charts = [], htmls = [], sqls = [], uis = [] }: MessageBodyInput) {
  return (
    <>
      {content && <Panel padding="sm" className="whitespace-pre-wrap break-words bg-paper text-sm text-ink">{content}</Panel>}
      {charts.map((chart, i) => <ChartRenderer key={`c${i}`} spec={chart} />)}
      {uis.map((ui, i) => ui.root ? <SafeUiRenderer key={ui.id ?? `u${i}`} root={ui.root} /> : null)}
      {htmls.map((html, i) => <HtmlRenderer key={`h${i}`} html={html} />)}
      {sqls.map((sql, i) => <SqlResultTable key={`s${i}`} {...sql} />)}
    </>
  );
}
