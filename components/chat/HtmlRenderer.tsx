'use client';
import { useMemo } from 'react';
import DOMPurify from 'isomorphic-dompurify';

export function HtmlRenderer({ html }: { html: string }) {
  const safe = useMemo(
    () =>
      DOMPurify.sanitize(html, {
        USE_PROFILES: { html: true },
        ADD_ATTR: ['style', 'class', 'target', 'rel'],
        FORBID_TAGS: ['script', 'iframe', 'object', 'embed', 'form'],
        FORBID_ATTR: ['onerror', 'onload', 'onclick'],
      }),
    [html],
  );
  return (
    <div
      className="folio-html my-2 overflow-x-auto rounded-xl border border-slate-800 bg-slate-950/60 p-4 text-sm text-slate-100"
      dangerouslySetInnerHTML={{ __html: safe }}
    />
  );
}
