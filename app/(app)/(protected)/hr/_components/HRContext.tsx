'use client';

import { createContext, useContext, useState, type ReactNode } from 'react';

interface HRContextValue {
  selectedHrId: string;
  setSelectedHrId: (id: string) => void;
}

const HRContext = createContext<HRContextValue | null>(null);

export function HRProvider({
  initialHrId,
  children,
}: {
  initialHrId: string;
  children: ReactNode;
}) {
  const [selectedHrId, setSelectedHrId] = useState(initialHrId);
  return (
    <HRContext.Provider value={{ selectedHrId, setSelectedHrId }}>
      {children}
    </HRContext.Provider>
  );
}

export function useHRContext(): HRContextValue {
  const ctx = useContext(HRContext);
  if (!ctx) throw new Error('useHRContext must be used inside <HRProvider>');
  return ctx;
}
