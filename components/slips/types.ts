import type { VisionModel } from '@/ai/loadVisionModels';

export type SlipKind = 'receipt' | 'book_bank';

export interface BookBankFields {
  bankName: string;
  bankBranch: string;
  accountNumber: string;
  accountName: string;
}

export interface SlipDraftFields {
  vendorName: string;
  vendorAddress: string;
  createdTo: string;
  createdToAddress: string;
  transactionDate: string;
  paymentMethod: 'cash' | 'credit_card' | 'transfer';
  subtotal: number;
  vatAmount: number;
  totalAmount: number;
}

export interface ExpenseDraft extends SlipDraftFields {
  payeeType: 'employee' | 'vendor';
  items: ItemRow[];
}

export interface SubmitState {
  visible: boolean;
  canConfirm: boolean;
  confirming: boolean;
  extractionState: ExtractionState;
  pendingFile: boolean;
  isBookBank: boolean;
  error: string | null;
  hint: 'ok' | 'transfer-needs-bookbank' | 'missing-fields';
  draft: ExpenseDraft | null;
  slipId: number | null;
}

export interface SlipUploadHandle {
  submit: () => Promise<void>;
  extract?: () => void;
}

export interface ParsedFields {
  vendorName?: string;
  vendorAddress?: string;
  createdTo?: string;
  createdToAddress?: string;
  transactionDate?: string;
  paymentMethod?: string;
  subtotal?: number;
  vatAmount?: number;
  totalAmount?: number;
  currency?: string;
  items?: ItemRow[];
  isCorrupted?: boolean;
  correctionNotes?: string;
  bankName?: string;
  bankBranch?: string;
  accountNumber?: string;
  accountName?: string;
  payee?: string;
  reference?: string;
}

export interface UploadValidation {
  ok: boolean;
  errors: Array<{ code: string; severity: 'error' | 'warning'; field?: string; message: string }>;
  warnings: Array<{ code: string; severity: 'error' | 'warning'; field?: string; message: string }>;
  retried: boolean;
  summary: string;
}

export interface UploadOk {
  slipId: number;
  status: 'pending' | 'confirmed';
  parsed: ParsedFields;
  confidence: number;
  mode: string;
  fileKey: string;
  fileUrl: string;
  mime: string;
  size: number;
  kind?: SlipKind;
  validation?: UploadValidation;
}

export type Phase = 'idle' | 'extracting' | 'confirming' | 'confirmed';
export type ExtractionState = 'pending' | 'running' | 'done';

export interface ItemRow {
  description: string;
  qty?: number;
  unitPrice?: number;
  amount: number;
}

export interface SlipOcrOpts {
  kind: SlipKind;
  evidenceOnly?: boolean;
  initialModels?: VisionModel[];
  currentUserId?: number;
  onSlipReady?: (slipId: number, kind: SlipKind, parsed: ParsedFields) => void;
  onSlipDiscarded?: (slipId: number, kind: SlipKind) => void;
}
