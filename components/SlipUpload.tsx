'use client';

import React, { forwardRef } from 'react';
import type { VisionModel } from '@/ai/loadVisionModels';
import { BookBankUpload } from './slips/BookBankUpload';
import { ReceiptUpload } from './slips/ReceiptUpload';
import type { BookBankFields, ParsedFields, SlipUploadHandle, SubmitState } from './slips/types';

export type {
  BookBankFields,
  SlipDraftFields,
  SlipUploadHandle,
  SubmitState,
  SlipKind,
} from './slips/types';
export type { ReceiptUploadProps } from './slips/ReceiptUpload';
export type { BookBankUploadProps } from './slips/BookBankUpload';

export interface SlipUploadProps {
  kind?: 'receipt' | 'book_bank';
  onConfirmed?: (result: {
    slipId: number;
    expenseId: number;
    status: string;
    waybillId?: string;
  }) => void;
  onSlipReady?: (slipId: number, kind: 'receipt' | 'book_bank', parsed: ParsedFields) => void;
  onSlipDiscarded?: (slipId: number, kind: 'receipt' | 'book_bank') => void;
  onPaymentChange?: (next: 'cash' | 'credit_card' | 'transfer') => void;
  currentUserId?: number;
  initialModels?: VisionModel[];
  bookBankSlipId?: number | null;
  bookBankFields?: BookBankFields;
  onBookBankFieldsChange?: (f: BookBankFields) => void;
  hideSubmitButton?: boolean;
  onSubmitStateChange?: (state: SubmitState) => void;
  draftWaybillId?: string | null;
  onDraftStarted?: (info: { waybillId: string; expenseId: number }) => void;
}

export const SlipUpload = forwardRef<SlipUploadHandle, SlipUploadProps>(function SlipUpload(
  props,
  ref,
) {
  const { kind = 'receipt', onSlipReady, onSlipDiscarded, ...rest } = props;
  if (kind === 'book_bank') {
    return (
      <BookBankUpload
        ref={ref}
        {...rest}
        onSlipReady={(id, _kind, parsed) => onSlipReady?.(id, 'book_bank', parsed)}
        onSlipDiscarded={(id) => onSlipDiscarded?.(id, 'book_bank')}
      />
    );
  }
  return (
    <ReceiptUpload
      ref={ref}
      {...rest}
      onSlipReady={(id, _kind, parsed) => onSlipReady?.(id, 'receipt', parsed)}
      onSlipDiscarded={(id) => onSlipDiscarded?.(id, 'receipt')}
    />
  );
});

export default SlipUpload;
