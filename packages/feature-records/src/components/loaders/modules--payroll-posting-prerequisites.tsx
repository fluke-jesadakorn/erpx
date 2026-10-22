'use client';
import dynamic from 'next/dynamic';
export const PayrollPostingPrerequisites=dynamic(()=>import('@erpx/feature-people_payroll/components/payroll-posting-prerequisites').then(module=>module.PayrollPostingPrerequisites));
export const PayrollPostingSettingsLink=dynamic(()=>import('@erpx/feature-people_payroll/components/payroll-posting-prerequisites').then(module=>module.PayrollPostingSettingsLink));
