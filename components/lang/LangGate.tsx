'use client';

import React, { useEffect, useState } from 'react';
import { LangPicker, setLang, STORAGE_KEY } from './LangPicker';

export function LangGate() {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const stored = (typeof window !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null) as 'de' | 'th' | null;
    if (!stored) {
      setOpen(true);
    }
  }, []);
  if (!open) return null;
  return (
    <LangPicker
      open={open}
      onPick={(lang) => {
        setLang(lang);
        setOpen(false);
      }}
    />
  );
}