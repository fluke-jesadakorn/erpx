'use client';
import dynamic from 'next/dynamic';
export const PayrollOtherRemittancesPanel=dynamic(()=>import('@erpx/feature-people_payroll/components/payroll-other-remittances-panel').then(module=>module.PayrollOtherRemittancesPanel));
