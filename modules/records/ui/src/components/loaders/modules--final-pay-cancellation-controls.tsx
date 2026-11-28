'use client';
import dynamic from 'next/dynamic';
export const FinalPayCancellationControls=dynamic(()=>import('@erpx/people-payroll-ui/components/final-pay-cancellation-controls').then(module=>module.FinalPayCancellationControls));
