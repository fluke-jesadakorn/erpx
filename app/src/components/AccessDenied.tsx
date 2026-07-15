import React from 'react';
import { ROLE_LABEL, type DisplayRoleName } from '@folio-lib/org/display';

interface AccessDeniedProps {
  roleName?: string;
  requiredAccess?: string;
}

export const AccessDenied: React.FC<AccessDeniedProps> = ({
  roleName,
  requiredAccess
}) => {
  const getRoleLabel = (role?: string) => {
    return ROLE_LABEL[role as DisplayRoleName] ?? role ?? 'Unknown';
  };

  return (
    <div className="flex items-center justify-center min-h-[400px] animate-fade-in">
      <div className="glass-panel p-10 sm:p-14 rounded-3xl border-rose-500/30 text-center max-w-lg relative overflow-hidden shadow-2xl">
        {/* Background glow */}
        <div className="absolute -top-20 -right-20 w-60 h-60 bg-rose-500/10 rounded-full blur-3xl pointer-events-none" />
        <div className="absolute -bottom-20 -left-20 w-60 h-60 bg-indigo-500/10 rounded-full blur-3xl pointer-events-none" />

        {/* Lock icon */}
        <div className="relative z-10">
          <div className="inline-flex items-center justify-center w-20 h-20 rounded-3xl bg-gradient-to-br from-rose-600/20 to-rose-900/30 border border-rose-500/30 mb-6 shadow-xl shadow-rose-950/30">
            <span className="text-4xl">🔒</span>
          </div>

          <h3 className="text-xl font-black text-white mb-2 tracking-tight">
            Insufficient Access Permissions
          </h3>
          <p className="text-sm text-slate-300 font-sans leading-relaxed">
            Access Denied — Insufficient Permissions
          </p>

          <div className="mt-6 p-4 bg-slate-950/80 rounded-2xl border border-slate-800 text-xs space-y-2.5">
            <div className="flex justify-between items-center">
              <span className="text-slate-400 font-mono">Current Role:</span>
              <span className="px-2.5 py-0.5 rounded-full bg-amber-500/10 text-amber-400 border border-amber-500/30 font-bold font-mono text-xs uppercase">
                {getRoleLabel(roleName)}
              </span>
            </div>
            {requiredAccess && (
              <div className="flex justify-between items-center">
                <span className="text-slate-400 font-mono">Required:</span>
                <span className="px-2.5 py-0.5 rounded-full bg-indigo-500/10 text-indigo-400 border border-indigo-500/30 font-bold font-mono text-xs uppercase">
                  {requiredAccess}
                </span>
              </div>
            )}
          </div>

          <p className="text-sm text-slate-500 mt-5 font-sans leading-relaxed">
            Please switch to a role with access to this section<br />
            or contact the administrator to request additional permissions
          </p>
        </div>
      </div>
    </div>
  );
};
