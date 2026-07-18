'use client';
import { ChartRenderer } from './ChartRenderer';
import { HtmlRenderer } from './HtmlRenderer';
import { SqlResultTable } from './SqlResultTable';
import type { ChartSpec } from '@/components/chat/chartContract';
import type { SqlResolved } from '@/chat/history';
import { T } from '@/components/i18n/T';

export interface MessageRenderInput {
  role: 'user' | 'assistant' | 'system';
  content: string;
  charts?: ChartSpec[];
  htmls?: string[];
  sqls?: SqlResolved[];
  modelName?: string | null;
  latencyMs?: number | null;
  pending?: boolean;
}

export function MessageRenderer({
  role,
  content,
  charts = [],
  htmls = [],
  sqls = [],
  modelName,
  latencyMs,
  pending,
}: MessageRenderInput) {
  const isUser = role === 'user';
  if (isUser) {
    return (
      <div className="ml-auto max-w-[85%] rounded-md border border-accent bg-accent-strong px-3 py-2 text-sm text-ink whitespace-pre-wrap break-words">
        {content}
      </div>
    );
  }
  const hasBlocks = charts.length + htmls.length + sqls.length > 0;
  return (
    <div className="mr-auto max-w-[92%]">
      {pending ? (
        <div className="rounded-md border border-rule bg-paper px-3 py-2 text-xs text-ink-2 font-mono animate-pulse">
          ⏳ <T id="chat.thinking" />
        </div>
      ) : (
        <>
          {content && !hasBlocks && (
            <div className="rounded-md border border-rule bg-paper px-3 py-2 text-sm text-ink whitespace-pre-wrap break-words">
              {content}
            </div>
          )}
          {charts.map((c, i) => (
            <ChartRenderer key={`c${i}`} spec={c} />
          ))}
          {htmls.map((h, i) => (
            <HtmlRenderer key={`h${i}`} html={h} />
          ))}
          {sqls.map((s, i) => (
            <SqlResultTable key={`s${i}`} {...s} />
          ))}
        </>
      )}
      {!pending && (modelName || latencyMs != null) && (
        <div className="mt-1 px-1 text-xs font-mono text-mute">
          {modelName ?? ''}
          {latencyMs != null ? ` · ${latencyMs}ms` : ''}
        </div>
      )}
    </div>
  );
}