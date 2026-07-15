'use client';

import React, { createContext, useContext, useEffect, useState } from 'react';
import type { SecondaryLocale } from '@folio-lib/server/locale';
import { STORAGE_KEY, LANG_EVENT } from '@/components/lang/LangPicker';

const Ctx = createContext<SecondaryLocale>('th');

export function SecondaryLocaleProvider({
  children,
}: {
  value?: SecondaryLocale;
  children: React.ReactNode;
}) {
  const [loc, setLoc] = useState<SecondaryLocale>('th');
  useEffect(() => {
    const read = (): SecondaryLocale => {
      const v = typeof window !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null;
      return v === 'de' || v === 'th' ? v : 'th';
    };
    setLoc(read());
    const handler = (e: Event) => {
      const d = (e as CustomEvent<SecondaryLocale>).detail;
      if (d === 'de' || d === 'th') setLoc(d);
      else setLoc(read());
    };
    window.addEventListener(LANG_EVENT, handler);
    const onStorage = (e: StorageEvent) => {
      if (e.key === STORAGE_KEY) setLoc(read());
    };
    window.addEventListener('storage', onStorage);
    return () => {
      window.removeEventListener(LANG_EVENT, handler);
      window.removeEventListener('storage', onStorage);
    };
  }, []);
  return <Ctx.Provider value={loc}>{children}</Ctx.Provider>;
}

export function useSecondaryLocale(): SecondaryLocale {
  return useContext(Ctx);
}
