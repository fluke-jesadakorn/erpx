'use client';

import React from 'react';
import { T } from '@/components/i18n/T';
import { ROLE_LABEL, type DisplayRoleName } from '@/org/display';

interface AccessDeniedProps {
  roleName?: string;
  requiredAccess?: string;
}

export const AccessDenied: React.FC<AccessDeniedProps> = ({
  roleName,
  requiredAccess,
}) => {
  const getRoleLabel = (role?: string) => {
    return ROLE_LABEL[role as DisplayRoleName] ?? role ?? 'Unknown';
  };

  return (
    <div className="flex items-center justify-center min-h-[400px] animate-fade-in">
      <div className="bg-paper-2 border border-rule p-10 sm:p-14 rounded-md border border-critical text-center max-w-lg relative overflow-hidden shadow-2xl">
        <div className="absolute -top-20 -right-20 w-60 h-60 bg-critical rounded-full blur-3xl pointer-events-none" />
        <div className="absolute -bottom-20 -left-20 w-60 h-60 bg-accent rounded-full blur-3xl pointer-events-none" />

        <div className="relative z-10">
          <div className="inline-flex items-center justify-center w-20 h-20 rounded-md  from-critical-strong to-critical-strong border border-critical mb-6 shadow-xl shadow-critical-strong">
            <span className="text-4xl">🔒</span>
          </div>

          <h3 className="text-xl font-black text-ink mb-2 tracking-tight">
            <T id="access.deniedTitle" />
          </h3>
          <p className="text-sm text-ink-2 font-sans leading-relaxed">
            <T id="access.deniedBody" />
          </p>

          <div className="mt-6 p-4 bg-paper-2/80 rounded-md border border-rule text-xs space-y-2.5">
            <div className="flex justify-between items-center">
              <span className="text-ink-2 font-mono"><T id="access.currentRole" /></span>
              <span className="px-2.5 py-0.5 rounded-full bg-caution text-caution border border-caution font-bold font-mono text-xs uppercase">
                {getRoleLabel(roleName)}
              </span>
            </div>
            {requiredAccess && (
              <div className="flex justify-between items-center">
                <span className="text-ink-2 font-mono"><T id="access.required" /></span>
                <span className="px-2.5 py-0.5 rounded-full bg-accent text-accent border border-accent font-bold font-mono text-xs uppercase">
                  {requiredAccess}
                </span>
              </div>
            )}
          </div>

          <p className="text-sm text-mute mt-5 font-sans leading-relaxed">
            <T id="access.deniedHelp" />
          </p>
        </div>
      </div>
    </div>
  );
};

export default AccessDenied;
