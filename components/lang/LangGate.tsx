'use client';

import React, { useEffect, useState } from 'react';
import { LangPicker, setLang, LANG_STORAGE_KEY } from './LangPicker';

export function LangGate() {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const stored = (typeof window !== 'undefined' ? localStorage.getItem(LANG_STORAGE_KEY) : null) as string | null;
    if (!stored) setOpen(true);
  }, []);
  if (!open) return null;
  return (
    <>
      <div className="fixed inset-0 z-toast bg-ink/55 backdrop-blur-sm animate-fade-in" aria-hidden />
      <div className="fixed inset-0 z-toast flex items-center justify-center p-4 animate-fade-in">
        <LangPicker
          open={open}
          onPick={(lang) => {
            setLang(lang);
            setOpen(false);
          }}
        />
      </div>
    </>
  );
}
