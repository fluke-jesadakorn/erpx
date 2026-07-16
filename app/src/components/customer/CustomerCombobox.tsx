'use client';

import React, { useEffect, useRef, useState } from 'react';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { Bilingual } from '@/components/i18n/Bilingual';
import custDict from '@folio-lib/i18n/customers';

export interface CustomerComboboxOption {
  id: number;
  code: string;
  name: string;
  credit_limit_thb?: number;
  outstanding_ar_thb?: number;
  blacklist?: boolean;
  payment_terms?: string;
}

export interface CustomerComboboxProps {
  value: number | null;
  onChange: (customerId: number | null, customer: CustomerComboboxOption | null) => void;
  disabled?: boolean;
  placeholder?: string;
  className?: string;
}

const DEBOUNCE_MS = 300;
const MAX_RESULTS = 10;

function pickText(key: string, _locale: 'th' | 'de'): { en: string; th?: string; de?: string } {
  const en = custDict.en[key] ?? key;
  const th = custDict.th?.[key];
  const de = custDict.de?.[key];
  return { en, th, de };
}

function t(key: string, locale: 'th' | 'de'): string {
  const dict = pickText(key, locale);
  return locale === 'de' ? (dict.de ?? dict.en) : (dict.th ?? dict.en);
}

function _fill(tmpl: string, vars: Record<string, string | number>): string {
  return tmpl.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`));
}

export function CustomerCombobox({
  value,
  onChange,
  disabled = false,
  placeholder,
  className,
}: CustomerComboboxProps): React.JSX.Element {
  const locale = useSecondaryLocale();
  const [query, setQuery] = useState<string>('');
  const [results, setResults] = useState<CustomerComboboxOption[]>([]);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<boolean>(false);
  const [selected, setSelected] = useState<CustomerComboboxOption | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const lastLoadedRef = useRef<number | null>(null);

  useEffect(() => {
    if (value == null) {
      lastLoadedRef.current = null;
      setSelected(null);
      return;
    }
    if (selected && selected.id === value) {
      lastLoadedRef.current = value;
      return;
    }
    if (lastLoadedRef.current === value) return;
    lastLoadedRef.current = value;
    let cancelled = false;
    (async () => {
      try {
        const r = await fetch(`/api/customers/${value}`, {
          credentials: 'include',
        });
        if (!r.ok) {
          if (!cancelled) {
            lastLoadedRef.current = null;
            setSelected(null);
          }
          return;
        }
        const data = await r.json().catch(() => null);
        const c = data?.customer ?? data?.data ?? null;
        if (cancelled || !c) return;
        setSelected({
          id: Number(c.id),
          code: String(c.code ?? ''),
          name: String(c.name ?? ''),
          credit_limit_thb: c.credit_limit_thb != null ? Number(c.credit_limit_thb) : undefined,
          outstanding_ar_thb: c.outstanding_ar_thb != null ? Number(c.outstanding_ar_thb) : undefined,
          blacklist: !!c.blacklist,
          payment_terms: c.payment_terms != null ? String(c.payment_terms) : undefined,
        });
      } catch {
        if (!cancelled) {
          lastLoadedRef.current = null;
          setSelected(null);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // selected is intentionally not a dep — we use lastLoadedRef to avoid loop
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  useEffect(() => {
    function onDocClick(e: MouseEvent) {
      if (!rootRef.current) return;
      if (!rootRef.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, []);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    abortRef.current?.abort();
    const q = query.trim();
    if (q.length < 1) {
      setResults([]);
      setLoading(false);
      setError(null);
      return;
    }
    setLoading(true);
    setError(null);
    debounceRef.current = setTimeout(async () => {
      const ctrl = new AbortController();
      abortRef.current = ctrl;
      try {
        const r = await fetch(
          `/api/customers/search?q=${encodeURIComponent(q)}&limit=${MAX_RESULTS}`,
          { credentials: 'include', signal: ctrl.signal },
        );
        if (!r.ok) {
          setError(`HTTP ${r.status}`);
          setResults([]);
          setLoading(false);
          return;
        }
        const data = await r.json().catch(() => null);
        const list = (data?.results ?? data?.data ?? []) as Array<Record<string, unknown>>;
        const mapped: CustomerComboboxOption[] = list.slice(0, MAX_RESULTS).map((c) => ({
          id: Number(c.id),
          code: String(c.code ?? ''),
          name: String(c.name ?? ''),
          credit_limit_thb: c.credit_limit_thb != null ? Number(c.credit_limit_thb) : undefined,
          outstanding_ar_thb: c.outstanding_ar_thb != null ? Number(c.outstanding_ar_thb) : undefined,
          blacklist: !!c.blacklist,
          payment_terms: c.payment_terms != null ? String(c.payment_terms) : undefined,
        }));
        setResults(mapped);
        setLoading(false);
      } catch (e: any) {
        if (e?.name === 'AbortError') return;
        setError(e?.message ?? t('customers.combo.search_failed', locale));
        setLoading(false);
      }
    }, DEBOUNCE_MS);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query, locale]);

  function pick(c: CustomerComboboxOption) {
    setSelected(c);
    setOpen(false);
    setQuery('');
    setResults([]);
    onChange(c.id, c);
  }

  function clear() {
    setSelected(null);
    setQuery('');
    setResults([]);
    onChange(null, null);
  }

  const creditLimit = selected?.credit_limit_thb ?? null;
  const outstanding = selected?.outstanding_ar_thb ?? null;
  const utilizationPct =
    creditLimit != null && creditLimit > 0 && outstanding != null
      ? Math.min(100, Math.round((outstanding / creditLimit) * 100))
      : null;
  const overCredit =
    utilizationPct != null && creditLimit != null && utilizationPct >= 80;

  const placeholderText = placeholder ?? t('customers.combo.placeholder', locale);
  const blacklistPill = pickText('customers.combo.blacklist_pill', locale);
  const changeText = pickText('customers.combo.change', locale);
  const searchingText = pickText('customers.combo.searching', locale);
  const errorText = pickText('customers.combo.error', locale);
  const emptyText = pickText('customers.combo.empty', locale);
  const creditLimitText = pickText('customers.combo.credit_limit', locale);
  const thbText = pickText('customers.combo.thb', locale);
  const arText = pickText('customers.combo.ar', locale);
  const utilText = pickText('customers.combo.util', locale);

  return (
    <div ref={rootRef} className={['relative w-full', className ?? ''].join(' ')}>
      {selected ? (
        <div className="flex flex-wrap items-center gap-2 rounded-xl border border-info/40 bg-info-soft/40 px-3 py-2">
          <span className="font-mono text-sm uppercase tracking-wider text-info-strong">
            {selected.code}
          </span>
          <span className="min-w-0 flex-1 truncate text-sm text-ink">{selected.name}</span>
          {selected.blacklist && (
            <span className="rounded-full border border-critical/50 bg-critical-soft px-2 py-0.5 text-xs font-mono uppercase tracking-wider text-critical-strong">
              <Bilingual en={blacklistPill.en} th={blacklistPill.th} de={blacklistPill.de} locale={locale} />
            </span>
          )}
          {selected.payment_terms && (
            <span className="rounded-full border border-rule bg-paper-3 px-2 py-0.5 text-xs font-mono text-ink-2">
              {selected.payment_terms}
            </span>
          )}
          <button
            type="button"
            onClick={clear}
            disabled={disabled}
            className="ml-auto rounded-md border border-rule bg-paper-3 px-2 py-1 text-xs font-mono text-ink-2 hover:border-critical/50 hover:text-critical transition-colors disabled:opacity-50"
            aria-label={t('customers.combo.clear', locale)}
            title={t('customers.combo.clear', locale)}
          >
            <Bilingual en={changeText.en} th={changeText.th} de={changeText.de} locale={locale} />
          </button>
        </div>
      ) : (
        <input
          type="text"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          placeholder={placeholderText}
          disabled={disabled}
          className="w-full rounded-lg border border-rule bg-paper-2 px-3 py-2 text-sm text-ink placeholder:text-mute/60 focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/25 disabled:opacity-50 transition-colors"
          aria-label={t('customers.combo.placeholder', locale)}
          autoComplete="off"
        />
      )}

      {open && !selected && query.trim().length >= 1 && (
        <ul
          className="absolute left-0 right-0 top-full z-30 mt-1 max-h-80 overflow-auto rounded-xl glass-panel-heavy border border-rule shadow-modal"
          role="listbox"
        >
          {loading && (
            <li className="px-3 py-2 text-sm font-mono text-ink-2">
              <span className="inline-block h-3 w-3 animate-spin rounded-full border-2 border-info border-t-transparent align-middle" />{' '}
              <Bilingual en={searchingText.en} th={searchingText.th} de={searchingText.de} locale={locale} />
            </li>
          )}
          {error && !loading && (
            <li className="px-3 py-2 text-sm text-critical">
              <Bilingual en={errorText.en} th={errorText.th} de={errorText.de} locale={locale} />
              {error && ` (${error})`}
            </li>
          )}
          {!loading && !error && results.length === 0 && (
            <li className="px-3 py-2 text-sm font-mono text-ink-2">
              <Bilingual en={emptyText.en} th={emptyText.th} de={emptyText.de} locale={locale} />
            </li>
          )}
          {!loading && results.map((c) => (
            <li key={c.id}>
              <button
                type="button"
                onClick={() => pick(c)}
                className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-paper-3 transition-colors"
                role="option"
                aria-selected={false}
              >
                <span className="font-mono text-sm text-info">{c.code}</span>
                <span className="min-w-0 flex-1 truncate text-sm text-ink">{c.name}</span>
                {c.blacklist && (
                  <span className="rounded-full border border-critical/50 bg-critical-soft px-1.5 py-0.5 text-xs font-mono text-critical-strong">
                    🚫
                  </span>
                )}
                {c.outstanding_ar_thb != null && c.credit_limit_thb != null && c.credit_limit_thb > 0 && (
                  <span
                    className={[
                      'rounded-full px-1.5 py-0.5 text-xs font-mono border',
                      c.outstanding_ar_thb / c.credit_limit_thb > 0.8
                        ? 'border-critical/50 bg-critical-soft text-critical-strong'
                        : 'border-rule bg-paper-3 text-ink-2',
                    ].join(' ')}
                  >
                    <Bilingual
                      en={`${Math.round((c.outstanding_ar_thb / c.credit_limit_thb) * 100)}% util`}
                      th={`${Math.round((c.outstanding_ar_thb / c.credit_limit_thb) * 100)}% ใช้ไป`}
                      de={`${Math.round((c.outstanding_ar_thb / c.credit_limit_thb) * 100)}% Auslastung`}
                      locale={locale}
                    />
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      )}

      {selected && creditLimit != null && (
        <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs font-mono text-ink-2">
          <span>
            <Bilingual en={creditLimitText.en} th={creditLimitText.th} de={creditLimitText.de} locale={locale} />{' '}
            <span className="text-ink tabular-nums">
              {creditLimit.toLocaleString('th-TH', { maximumFractionDigits: 0 })}
            </span>{' '}
            <Bilingual en={thbText.en} th={thbText.th} de={thbText.de} locale={locale} />
          </span>
          {outstanding != null && (
            <>
              <span className="text-mute">·</span>
              <span>
                <Bilingual en={arText.en} th={arText.th} de={arText.de} locale={locale} />{' '}
                <span
                  className={[
                    'tabular-nums',
                    overCredit ? 'text-critical' : 'text-caution',
                  ].join(' ')}
                >
                  {outstanding.toLocaleString('th-TH', { maximumFractionDigits: 0 })}
                </span>{' '}
                <Bilingual en={thbText.en} th={thbText.th} de={thbText.de} locale={locale} />
              </span>
              <span className="text-mute">·</span>
              <span>
                <Bilingual en={utilText.en} th={utilText.th} de={utilText.de} locale={locale} />{' '}
                <span
                  className={[
                    'font-bold tabular-nums',
                    overCredit ? 'text-critical' : 'text-caution',
                  ].join(' ')}
                >
                  {utilizationPct}%
                </span>
              </span>
            </>
          )}
        </div>
      )}
    </div>
  );
}

export default CustomerCombobox;
