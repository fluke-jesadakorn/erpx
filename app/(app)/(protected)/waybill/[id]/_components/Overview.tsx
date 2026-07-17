import React from 'react';
import { WaybillTabs } from './WaybillTabs';

interface Props {
  waybillId: string;
  active: 'overview' | 'audit' | 'gl' | 'attachments' | 'chat';
  children: React.ReactNode;
}

export function OverviewShell({ waybillId, active, children }: Props) {
  return (
    <div className="space-y-6">
      <WaybillTabs waybillId={waybillId} active={active} />
      {children}
    </div>
  );
}

export default OverviewShell;