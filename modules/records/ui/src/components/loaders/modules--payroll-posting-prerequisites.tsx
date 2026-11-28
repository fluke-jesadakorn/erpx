'use client';
import dynamic from 'next/dynamic';
export const PayrollPostingPrerequisites=dynamic(()=>import('@erpx/people-payroll-ui/components/payroll-posting-prerequisites').then(module=>module.PayrollPostingPrerequisites));
export const PayrollPostingSettingsLink=dynamic(()=>import('@erpx/people-payroll-ui/components/payroll-posting-prerequisites').then(module=>module.PayrollPostingSettingsLink));
