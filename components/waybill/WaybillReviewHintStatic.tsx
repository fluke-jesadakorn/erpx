import type { ReactNode } from 'react';
import { generateReviewHintAction } from '@/app/actions/waybillReview';
import { T } from '@/components/i18n/TServer';
import { Button } from '@/components/ui/Button';

interface Props {
  waybillId: string;
  stage: 'hod' | 'am';
  lang: 'en' | 'th' | 'de';
  label: ReactNode;
  persistedHint: string | null;
  generatedAt: string | null;
}

async function submitReviewHint(formData: FormData): Promise<void> {
  'use server';
  await generateReviewHintAction(null, formData);
}

export async function WaybillReviewHintStatic({
  waybillId,
  stage,
  lang,
  label,
  persistedHint,
  generatedAt,
}: Props) {
  return (
    <section className="rounded-md border border-info bg-info-strong">
      <form action={submitReviewHint}>
        <input type="hidden" name="waybillId" value={waybillId} />
        <input type="hidden" name="stage" value={stage} />
        <input type="hidden" name="lang" value={lang} />
        <div className="flex items-center gap-2 p-4">
          <h3 className="flex items-center gap-1.5 text-xs uppercase tracking-widest text-info">
            {label}
          </h3>
          <Button
            type="submit"
            variant="ghost"
            size="sm"
            className="ml-auto border border-info bg-info text-xs font-mono text-info-soft hover:bg-info"
          >
            <T id={persistedHint ? 'waybill.review.refresh' : 'waybill.review.generate'} />
          </Button>
        </div>
        <div className="border-t border-info p-4">
          {persistedHint ? (
            <p
              className="whitespace-pre-wrap text-sm leading-relaxed text-ink"
              title={generatedAt ?? undefined}
            >
              {persistedHint}
            </p>
          ) : (
            <p className="text-xs text-[var(--text-faint)]">
              <T id="waybill.review.placeholder" />
            </p>
          )}
        </div>
      </form>
    </section>
  );
}
