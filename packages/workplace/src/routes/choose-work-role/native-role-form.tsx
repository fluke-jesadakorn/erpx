'use client';

import { useState, type FormEvent, type ReactNode } from 'react';
import { Button, InlineNotice, Stack } from '@erpx/ui/components/foundation';

export default function NativeRoleForm({ endpoint, next, companyId, assignmentId, assignmentVersion, revision, locale, children }: {
  endpoint: string;
  next: string;
  companyId: string;
  assignmentId: string;
  assignmentVersion: number;
  revision: number;
  locale: 'en' | 'th';
  children: ReactNode;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const th = locale === 'th';
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError('');
    try {
      const target = new URL(next, window.location.origin);
      if (target.origin !== window.location.origin || target.pathname !== '/launch') throw new Error('Invalid work role destination');
      const response = await fetch(endpoint, {
        method: 'POST', credentials: 'same-origin', cache: 'no-store', redirect: 'error',
        headers: { 'Content-Type': 'application/json', 'x-erpx-requested-company': companyId },
        body: JSON.stringify({ assignmentId, assignmentVersion, revision }),
      });
      if (!response.ok) {
        if (response.status === 409) {
          window.location.reload();
          return;
        }
        throw new Error('Native role selection failed');
      }
      window.location.assign(target.href);
    } catch {
      setError(th ? 'เลือกบทบาทไม่ได้ โปรดลองอีกครั้ง' : 'Could not select the work role. Try again.');
      setBusy(false);
    }
  }
  return <form onSubmit={submit}><Stack>
    {children}
    {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
    <div><Button appearance="standard" type="submit" disabled={busy}>{th ? 'ใช้บทบาทนี้' : 'Use this role'}</Button></div>
  </Stack></form>;
}
