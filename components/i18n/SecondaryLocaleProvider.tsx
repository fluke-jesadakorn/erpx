'use client';

import React, { createContext, useContext, useEffect, useState } from 'react';
import {
  SECONDARIES,
  STORAGE_KEY,
  LANG_EVENT,
  type SecondaryLocale,
} from '@/i18n/config';

const Ctx = createContext<SecondaryLocale>('th');

interface ProviderProps {
  initial?: SecondaryLocale;
  children: React.ReactNode;
}

export function SecondaryLocaleProvider({ initial, children }: ProviderProps) {
  const [loc, setLoc] = useState<SecondaryLocale>(initial ?? 'th');

  useEffect(() => {
    const v = (typeof window !== 'undefined' ? window.localStorage.getItem(STORAGE_KEY) : null) as SecondaryLocale | null;
    if (v && (SECONDARIES as readonly string[]).includes(v)) setLoc(v);
    const onEvent = (e: Event) => {
      const d = (e as CustomEvent<SecondaryLocale>).detail;
      if (d && (SECONDARIES as readonly string[]).includes(d)) setLoc(d);
    };
    const onStorage = (e: StorageEvent) => {
      if (e.key !== STORAGE_KEY) return;
      const nv = e.newValue;
      if (nv && (SECONDARIES as readonly string[]).includes(nv)) setLoc(nv as SecondaryLocale);
    };
    window.addEventListener(LANG_EVENT, onEvent);
    window.addEventListener('storage', onStorage);
    return () => {
      window.removeEventListener(LANG_EVENT, onEvent);
      window.removeEventListener('storage', onStorage);
    };
  }, []);

  return <Ctx.Provider value={loc}>{children}</Ctx.Provider>;
}

export function useSecondaryLocale(): SecondaryLocale {
  return useContext(Ctx);
}
