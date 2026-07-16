'use client';

import React, { createContext, useContext, useEffect, useState } from 'react';

export interface ActorPolicies {
  canSeeGlLines: boolean;
  canPostAccrual: boolean;
  canPostSettlement: boolean;
  canConfirmGl: boolean;
  canSettleExpense: boolean;
  canFinalApprove: boolean;
  canAttach: boolean;
  canRemoveAttachment: boolean;
  canRecall: boolean;
  canAct: boolean;
}

export interface ActorSnapshot {
  id: number;
  fullname?: string;
  employee_code?: string;
  role_name?: string;
  locale?: 'th' | 'de';
  policies?: ActorPolicies;
  [k: string]: unknown;
}

interface ActorCtx {
  actor: ActorSnapshot | null;
  setActor: (a: ActorSnapshot | null) => void;
}

const Ctx = createContext<ActorCtx | null>(null);

export const ActorProvider: React.FC<{
  initialActor?: ActorSnapshot | null;
  value?: ActorSnapshot | null;
  children: React.ReactNode;
}> = ({ initialActor, value, children }) => {
  const resolved = value !== undefined ? value : initialActor ?? null;
  const [actor, setActor] = useState<ActorSnapshot | null>(resolved);

  useEffect(() => {
    setActor(value !== undefined ? value : initialActor ?? null);
  }, [value, initialActor]);

  useEffect(() => {
    function onChange(e: Event) {
      const ce = e as CustomEvent<{ actor: ActorSnapshot | null }>;
      setActor(ce.detail?.actor ?? null);
    }
    window.addEventListener('folio:actor-changed', onChange);
    return () => window.removeEventListener('folio:actor-changed', onChange);
  }, []);

  return <Ctx.Provider value={{ actor, setActor }}>{children}</Ctx.Provider>;
};

export function useActor(): ActorCtx {
  const v = useContext(Ctx);
  if (!v) throw new Error('useActor must be used inside <ActorProvider>');
  return v;
}
