'use client';

import React, { useEffect, useState } from 'react';
import { useT } from '@/components/i18n/useT';
import { T } from '@/components/i18n/T';
import chromeDict from '@folio-lib/i18n/chrome';

const MIN_VISIBLE_MS = 400;
const HARD_FALLBACK_MS = 3000;

export const GlobalLoading: React.FC = () => {
  const [visible, setVisible] = useState(true);
  const t = useT(chromeDict);
  const text = t('chrome.loading');

  useEffect(() => {
    const startedAt = Date.now();
    let finished = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const finish = () => {
      if (finished) return;
      finished = true;
      const elapsed = Date.now() - startedAt;
      const wait = Math.max(0, MIN_VISIBLE_MS - elapsed);
      timer = setTimeout(() => {
        setVisible(false);
        if (typeof document !== 'undefined') {
          document.documentElement.classList.remove('gl-loading');
        }
      }, wait);
    };

    if (document.readyState === 'complete') {
      finish();
    } else {
      window.addEventListener('load', finish, { once: true });
      timer = setTimeout(finish, HARD_FALLBACK_MS);
    }

    return () => {
      window.removeEventListener('load', finish);
      if (timer) clearTimeout(timer);
    };
  }, []);

  if (!visible) return null;

  const swallow = (e: React.SyntheticEvent) => {
    e.stopPropagation();
    e.preventDefault();
  };

  return (
    <div
      role="status"
      aria-live="polite"
      aria-busy="true"
      onClick={swallow}
      onMouseDown={swallow}
      onKeyDown={swallow}
      onTouchStart={swallow}
      className="fixed inset-0 z-[60] bg-slate-950/40 backdrop-blur-sm flex items-center justify-center transition-opacity duration-150"
    >
      <div className="glass-panel rounded-2xl px-6 py-4 flex items-center gap-3 shadow-2xl">
        <svg
          className="animate-spin h-6 w-6 text-indigo-300 motion-reduce:animate-none"
          viewBox="0 0 24 24"
          fill="none"
          aria-hidden="true"
        >
          <circle
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeOpacity="0.25"
            strokeWidth="4"
          />
          <path
            d="M22 12a10 10 0 0 1-10 10"
            stroke="currentColor"
            strokeWidth="4"
            strokeLinecap="round"
          />
        </svg>
        <span className="text-sm font-mono font-bold text-slate-200 uppercase tracking-wider">
          <T value={text} />
        </span>
      </div>
    </div>
  );
};

export default GlobalLoading;
