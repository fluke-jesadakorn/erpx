// Post-parse validation for OCR output.
//
// The vision LLM is asked to return strict JSON, but smaller models hallucinate:
//   - vendor name is invented when text is blurry
//   - subtotal/VAT/total are guessed to balance the equation
//   - paymentMethod becomes "cash" by default even when nothing was read
//   - items[] is filled with plausible-but-fabricated rows
//
// These validators catch those cases by checking field shape + math + domain
// plausibility. They never modify values — only flag. The caller decides whether
// to retry the LLM call or stamp isCorrupted=true.

export type Severity = 'error' | 'warning';

export interface ValidationIssue {
  code: string;
  severity: Severity;
  field?: string;
  message: string;
}

export interface ValidationResult {
  ok: boolean;
  errors: ValidationIssue[];
  warnings: ValidationIssue[];
  summary: string;
}

const EMPTY: ValidationResult = { ok: true, errors: [], warnings: [], summary: '' };

const THAI_RANGE = /[\u0E00-\u0E7F]/;
const LATIN_RANGE = /[A-Za-z]/;

function hasLetters(s: unknown): boolean {
  return typeof s === 'string' && (THAI_RANGE.test(s) || LATIN_RANGE.test(s));
}

function asNumber(v: unknown): number | null {
  if (typeof v === 'number' && Number.isFinite(v)) return v;
  if (typeof v === 'string') {
    const cleaned = v.replace(/[, ]/g, '').trim();
    if (!cleaned) return null;
    const n = Number(cleaned);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

export function validateReceipt(parsed: Record<string, unknown>): ValidationResult {
  if (parsed.isCorrupted === true) {
    return {
      ok: false,
      errors: [{
        code: 'model_flagged_corrupted',
        severity: 'error',
        message: 'Model already flagged isCorrupted=true; skipped further checks.',
      }],
      warnings: [],
      summary: 'model-flagged',
    };
  }

  const errors: ValidationIssue[] = [];
  const warnings: ValidationIssue[] = [];

  // vendorName
  const vendor = parsed.vendorName;
  if (!vendor || typeof vendor !== 'string' || !vendor.trim()) {
    errors.push({ code: 'vendor_missing', severity: 'error', field: 'vendorName', message: 'vendorName is empty.' });
  } else {
    const t = vendor.trim();
    if (t.length < 2) {
      errors.push({ code: 'vendor_too_short', severity: 'error', field: 'vendorName', message: `vendorName too short (${t.length} chars).` });
    }
    if (t.length > 100) {
      errors.push({ code: 'vendor_too_long', severity: 'error', field: 'vendorName', message: 'vendorName too long (>100 chars).' });
    }
    if (!hasLetters(t)) {
      errors.push({ code: 'vendor_gibberish', severity: 'error', field: 'vendorName', message: 'vendorName has no Thai or Latin letters.' });
    }
  }

  // createdTo
  const createdTo = parsed.createdTo;
  if (createdTo != null && typeof createdTo === 'string') {
    const t = createdTo.trim();
    if (t.length > 100) {
      errors.push({ code: 'created_to_too_long', severity: 'error', field: 'createdTo', message: 'createdTo too long (>100 chars).' });
    }
  }

  // vendorAddress
  const vendorAddress = parsed.vendorAddress;
  if (vendorAddress != null && typeof vendorAddress === 'string') {
    const t = vendorAddress.trim();
    if (t.length > 300) {
      errors.push({ code: 'vendor_address_too_long', severity: 'error', field: 'vendorAddress', message: 'vendorAddress too long (>300 chars).' });
    }
  }

  // createdToAddress
  const createdToAddress = parsed.createdToAddress;
  if (createdToAddress != null && typeof createdToAddress === 'string') {
    const t = createdToAddress.trim();
    if (t.length > 300) {
      errors.push({ code: 'created_to_address_too_long', severity: 'error', field: 'createdToAddress', message: 'createdToAddress too long (>300 chars).' });
    }
  }

  // transactionDate
  const dateRaw = parsed.transactionDate;
  if (!dateRaw || typeof dateRaw !== 'string' || !dateRaw.trim()) {
    errors.push({ code: 'date_missing', severity: 'error', field: 'transactionDate', message: 'transactionDate is empty.' });
  } else {
    const d = new Date(dateRaw);
    if (Number.isNaN(d.getTime())) {
      errors.push({ code: 'date_invalid', severity: 'error', field: 'transactionDate', message: `transactionDate unparseable: "${dateRaw}".` });
    } else {
      const now = Date.now();
      const oneDay = 24 * 60 * 60 * 1000;
      const futureLimit = now + 30 * oneDay;
      const pastLimit = now - 730 * oneDay;
      if (d.getTime() > futureLimit) {
        errors.push({ code: 'date_too_far_future', severity: 'error', field: 'transactionDate', message: `transactionDate > 30 days in the future (${dateRaw}).` });
      } else if (d.getTime() > now + oneDay) {
        warnings.push({ code: 'date_in_near_future', severity: 'warning', field: 'transactionDate', message: `transactionDate is in the future (${dateRaw}).` });
      }
      if (d.getTime() < pastLimit) {
        warnings.push({ code: 'date_too_old', severity: 'warning', field: 'transactionDate', message: `transactionDate > 2 years ago (${dateRaw}).` });
      }
    }
  }

  // numeric fields
  const subtotal = asNumber(parsed.subtotal);
  const vatAmount = asNumber(parsed.vatAmount);
  const totalAmount = asNumber(parsed.totalAmount);

  if (subtotal == null) {
    errors.push({ code: 'subtotal_invalid', severity: 'error', field: 'subtotal', message: 'subtotal is not a number.' });
  } else if (subtotal < 0) {
    errors.push({ code: 'subtotal_negative', severity: 'error', field: 'subtotal', message: 'subtotal is negative.' });
  } else if (subtotal > 10_000_000) {
    errors.push({ code: 'subtotal_unrealistic', severity: 'error', field: 'subtotal', message: `subtotal > 10M THB (${subtotal}).` });
  }

  if (vatAmount == null) {
    errors.push({ code: 'vat_invalid', severity: 'error', field: 'vatAmount', message: 'vatAmount is not a number.' });
  } else if (vatAmount < 0) {
    errors.push({ code: 'vat_negative', severity: 'error', field: 'vatAmount', message: 'vatAmount is negative.' });
  } else if (vatAmount > 10_000_000) {
    errors.push({ code: 'vat_unrealistic', severity: 'error', field: 'vatAmount', message: `vatAmount > 10M THB (${vatAmount}).` });
  }

  if (totalAmount == null) {
    errors.push({ code: 'total_invalid', severity: 'error', field: 'totalAmount', message: 'totalAmount is not a number.' });
  } else if (totalAmount < 0) {
    errors.push({ code: 'total_negative', severity: 'error', field: 'totalAmount', message: 'totalAmount is negative.' });
  } else if (totalAmount > 10_000_000) {
    errors.push({ code: 'total_unrealistic', severity: 'error', field: 'totalAmount', message: `totalAmount > 10M THB (${totalAmount}).` });
  }

  // math: subtotal + vat ≈ total
  if (subtotal != null && vatAmount != null && totalAmount != null) {
    const expected = round2(subtotal + vatAmount);
    const diff = Math.abs(expected - round2(totalAmount));
    if (diff > 0.01) {
      errors.push({
        code: 'math_broken',
        severity: 'error',
        field: 'totalAmount',
        message: `subtotal (${subtotal}) + vat (${vatAmount}) = ${expected}, but totalAmount = ${totalAmount} (diff ${diff.toFixed(2)}).`,
      });
    }
  }

  // VAT ratio plausibility (Thai VAT usually 7%, sometimes 0% or 10%)
  if (subtotal != null && subtotal > 0 && vatAmount != null && vatAmount > 0) {
    const ratio = vatAmount / subtotal;
    if (ratio > 0.20) {
      warnings.push({
        code: 'vat_ratio_high',
        severity: 'warning',
        field: 'vatAmount',
        message: `VAT ratio ${(ratio * 100).toFixed(1)}% > 20% (Thai VAT is usually 7%).`,
      });
    } else if (ratio < 0.05 && ratio > 0) {
      warnings.push({
        code: 'vat_ratio_low',
        severity: 'warning',
        field: 'vatAmount',
        message: `VAT ratio ${(ratio * 100).toFixed(1)}% < 5% but vatAmount > 0.`,
      });
    }
  }

  // paymentMethod enum
  const payment = parsed.paymentMethod;
  if (payment != null && typeof payment === 'string' && payment !== '') {
    if (!['cash', 'credit_card', 'transfer'].includes(payment)) {
      warnings.push({
        code: 'payment_unknown',
        severity: 'warning',
        field: 'paymentMethod',
        message: `paymentMethod "${payment}" is not in {cash, credit_card, transfer}; will be defaulted downstream.`,
      });
    }
  }

  // items[]
  const items = parsed.items;
  if (items != null) {
    if (!Array.isArray(items)) {
      errors.push({ code: 'items_not_array', severity: 'error', field: 'items', message: 'items is not an array.' });
    } else {
      const seenDescriptions = new Set<string>();
      let itemsSum = 0;
      let itemsCounted = 0;
      for (let i = 0; i < items.length; i++) {
        const it = items[i] as Record<string, unknown>;
        if (!it || typeof it !== 'object') {
          errors.push({ code: 'items_bad_row', severity: 'error', field: 'items', message: `items[${i}] is not an object.` });
          continue;
        }
        const desc = it.description;
        if (typeof desc !== 'string' || !desc.trim()) {
          errors.push({ code: 'items_bad_description', severity: 'error', field: 'items', message: `items[${i}].description is empty.` });
        } else {
          const key = desc.trim().toLowerCase();
          if (seenDescriptions.has(key)) {
            warnings.push({ code: 'items_duplicate', severity: 'warning', field: 'items', message: `items[${i}].description duplicates an earlier row ("${desc}").` });
          }
          seenDescriptions.add(key);
        }
        const amt = asNumber(it.amount);
        if (amt == null) {
          errors.push({ code: 'items_bad_amount', severity: 'error', field: 'items', message: `items[${i}].amount is not a number.` });
        } else if (amt < 0) {
          errors.push({ code: 'items_negative_amount', severity: 'error', field: 'items', message: `items[${i}].amount is negative (${amt}).` });
        } else if (amt > 10_000_000) {
          errors.push({ code: 'items_unrealistic_amount', severity: 'error', field: 'items', message: `items[${i}].amount > 10M (${amt}).` });
        } else {
          itemsSum += amt;
          itemsCounted++;
        }
      }

      // items-sum cross-check: sum(items.amount) ≈ subtotal
      if (itemsCounted > 0 && subtotal != null && subtotal > 0) {
        const tol = Math.max(0.05, subtotal * 0.02);
        const diff = Math.abs(round2(itemsSum) - round2(subtotal));
        if (diff > tol) {
          warnings.push({
            code: 'items_sum_mismatch',
            severity: 'warning',
            field: 'items',
            message: `sum(items.amount) = ${round2(itemsSum)} differs from subtotal = ${subtotal} by ${diff.toFixed(2)} (tolerance ${tol.toFixed(2)}).`,
          });
        }
      }
    }
  }

  return {
    ok: errors.length === 0,
    errors,
    warnings,
    summary: errors.length === 0
      ? warnings.length === 0 ? 'ok' : 'ok-with-warnings'
      : `errors=${errors.length}${warnings.length ? ` warnings=${warnings.length}` : ''}`,
  };
}

export function validateBookBank(parsed: Record<string, unknown>): ValidationResult {
  if (parsed.isCorrupted === true) {
    return {
      ok: false,
      errors: [{
        code: 'model_flagged_corrupted',
        severity: 'error',
        message: 'Model already flagged isCorrupted=true; skipped further checks.',
      }],
      warnings: [],
      summary: 'model-flagged',
    };
  }

  const errors: ValidationIssue[] = [];
  const warnings: ValidationIssue[] = [];

  const bankName = parsed.bankName;
  if (!bankName || typeof bankName !== 'string' || !bankName.trim()) {
    errors.push({ code: 'bank_name_missing', severity: 'error', field: 'bankName', message: 'bankName is empty.' });
  } else if (!hasLetters(bankName)) {
    errors.push({ code: 'bank_name_gibberish', severity: 'error', field: 'bankName', message: 'bankName has no Thai or Latin letters.' });
  }

  const branch = parsed.bankBranch;
  if (branch != null && typeof branch === 'string' && branch.length > 100) {
    warnings.push({ code: 'bank_branch_too_long', severity: 'warning', field: 'bankBranch', message: 'bankBranch > 100 chars.' });
  }

  const accountNumber = parsed.accountNumber;
  if (!accountNumber || typeof accountNumber !== 'string' || !accountNumber.trim()) {
    errors.push({ code: 'account_number_missing', severity: 'error', field: 'accountNumber', message: 'accountNumber is empty.' });
  } else {
    const digits = accountNumber.replace(/[^\d]/g, '');
    if (digits.length < 6) {
      errors.push({ code: 'account_number_too_short', severity: 'error', field: 'accountNumber', message: `accountNumber has only ${digits.length} digits (min 6).` });
    } else if (digits.length > 14) {
      errors.push({ code: 'account_number_too_long', severity: 'error', field: 'accountNumber', message: `accountNumber has ${digits.length} digits (max 14).` });
    } else if (digits !== accountNumber.replace(/\s/g, '').replace(/-/g, '')) {
      warnings.push({ code: 'account_number_has_separators', severity: 'warning', field: 'accountNumber', message: 'accountNumber contains non-digit chars (will be stripped downstream).' });
    }
  }

  const accountName = parsed.accountName;
  if (!accountName || typeof accountName !== 'string' || !accountName.trim()) {
    errors.push({ code: 'account_name_missing', severity: 'error', field: 'accountName', message: 'accountName is empty.' });
  } else {
    const t = accountName.trim();
    if (t.length < 2) {
      errors.push({ code: 'account_name_too_short', severity: 'error', field: 'accountName', message: `accountName too short (${t.length} chars).` });
    }
    if (!hasLetters(t)) {
      errors.push({ code: 'account_name_gibberish', severity: 'error', field: 'accountName', message: 'accountName has no Thai or Latin letters.' });
    }
  }

  return {
    ok: errors.length === 0,
    errors,
    warnings,
    summary: errors.length === 0
      ? warnings.length === 0 ? 'ok' : 'ok-with-warnings'
      : `errors=${errors.length}${warnings.length ? ` warnings=${warnings.length}` : ''}`,
  };
}

export function issuesToText(issues: ValidationIssue[]): string {
  if (issues.length === 0) return '';
  return issues.map(i => `[${i.code}${i.field ? `:${i.field}` : ''}] ${i.message}`).join(' | ');
}

export function isRetryableValidation(issues: ValidationIssue[]): boolean {
  // Don't retry when the LLM returned total nonsense (e.g. a non-date string the
  // validator flagged). The retry prompt can only fix plausibility / math / vendor
  // length / items-shape issues — not raw garbage. Be conservative.
  const nonRetryable = new Set([
    'vendor_missing',
    'date_missing',
    'date_invalid',
    'subtotal_invalid',
    'vat_invalid',
    'total_invalid',
    'bank_name_missing',
    'bank_name_gibberish',
    'account_number_missing',
    'account_name_missing',
    'account_name_gibberish',
  ]);
  return issues.every(i => !nonRetryable.has(i.code));
}

export { EMPTY as _EMPTY_VALIDATION };