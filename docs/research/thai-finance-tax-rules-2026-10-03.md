# Thai ordinary Finance tax adapter — source check 2026-10-03

This note records the primary evidence used by `src/lib/finance/thai-finance-tax-pack.ts` and the engineering tests. It is separate from the Thai employment-payroll rule pack. The adapter handles reviewed domestic THB supplier income under ordinary withholding, independently reviewed legal identities, immediate retained 50 ทวิ copies, and native itemized VAT invoices. It does not submit registered e-Tax or assert Revenue Department/provider certification.

## Ordinary withholding

The consolidated Revenue Department Order ท.ป.4/2528 supplies the supported ordinary categories: service/hire-of-work/professional income 3%, rent 5%, advertising 2%, transportation and corporate non-life-insurance premiums 1%. The contract threshold is THB 1,000, including an installment under that amount when the complete contract reaches the threshold. Classification needs the actual contract and recipient type; a generic purchase is not evidence that every agricultural, export or other special income is outside withholding. The adapter requires an independent reviewer and rejects unsupported foreign-payee, gross-up and provider branches. [Revenue Department Order ท.ป.4/2528](https://www.rd.go.th/3479.html).

The reviewed amount comes from immutable recognized invoice income, excludes its separately retained VAT, and allocates original income cumulatively across gross installments. Tax rounding uses integer satang. The independent test worksheet checks THB 1,000 service × 3% = 30 and THB 1,500.03 rent × 5% = 75.0015, rounded to 75; total withheld 105. For original income 1,000.17 over three gross installments, cumulative withholding rounds to 30.01 and installment differences are 10.00, 10.00 and 10.01. These worksheets do not call the production tax calculator.

The rule revision `TH-WHT-2026-10-03-r1` is deliberately pinned to ordinary payment dates in calendar 2026. This interval is the adapter's reviewed readiness interval, not a claim that the ordinary law expires on December 31. Income and tax source evidence remain immutable when later rates or identities change.

## Certificate and invoice issuance

Section 50 ทวิ requires two matching recipient certificates, with non-payroll withholding in the supported section 3 เตรส branch issued immediately on each withholding. Payroll certificates have separate timing and are not substituted by this adapter. [Revenue Code section 50 ทวิ](https://www.rd.go.th/5937.html).

Director-General Announcement 62 and its amendments govern the certificate's mandatory identity, income, tax, copy labels and sequential numbering. Amendment 100 permits a signature reproduced by computer from a real signature scan. Amendment 121 permits the relevant income categories to be printed. The implementation retains two recipient copies and a payer copy, a verified bound signature image, original source versions and exact HTML SHA256; independent identity amendments issue another retained edition. Correcting financial amounts uses the real source reversal, never an edit of the issued bytes. [Announcement 62](https://www.rd.go.th/3171.html), [Amendment 100](https://www.rd.go.th/2914.html), [Amendment 121](https://www.rd.go.th/12928.html), [official certificate guide](https://www.rd.go.th/fileadmin/images/image_law/images/guide100248.pdf).

A full VAT invoice needs the registered issuer, recipient identity, sequence, actual item description/quantity/value, distinct VAT, date and the prescribed document heading. The native adapter uses immutable multi-line quantities and reviewed registration evidence. An identity-only replacement changes metadata while retaining amounts and the posted journal; amount adjustments require their credit/debit-note source workflow. [Revenue Code sections 86/4, 86/9 and 86/10](https://www.rd.go.th/5208.html).

## Effective VAT and electronic withholding evidence

Decree 799 applies to the earlier VAT period. The visually inspected Gazette text of Decree 807, published August 23, 2026, applies from October 1, 2026 through September 30, 2027. The national component is 6.3%; the Revenue Department's accompanying announcement describes 7% including local tax. The adapter pins both effective intervals and requires 7% native VAT lines. [Decree 799](https://www.rd.go.th/fileadmin/user_upload/kormor/newlaw/dc799.pdf), [enacted Decree 807](https://www.rd.go.th/fileadmin/user_upload/kormor/newlaw/dc807.pdf), [Revenue Department announcement 18/2569](https://www.rd.go.th/fileadmin/user_upload/news/2569thai/news18_2569.pdf).

Revenue Department announcement 14/2569 describes Cabinet approval of a **draft** electronic-withholding concession for qualifying e-Withholding Tax payments in 2026–2027. This research did not establish its enacted instrument and the project has no registered provider filing adapter. The ordinary adapter therefore does not infer a 1% concession from an electronic file or synthetic bank result. This is a readiness boundary, not a claim that the announced concession cannot apply through a duly supported provider. [Revenue Department announcement 14/2569](https://www.rd.go.th/fileadmin/user_upload/news/2569thai/news14_2569.pdf), [current Revenue Department legislation index](https://www.rd.go.th/21221.html), [consolidated ministerial regulations](https://www.rd.go.th/6160.html).

## Source byte fingerprints

Downloaded original bytes were checked on 2026-10-03. Gazette PDF dates/rates were also inspected as rendered pages because embedded text extraction was incomplete. The rule-pack digest additionally pins the adapter configuration and these evidence records.

| Primary source | SHA256 of original bytes |
| --- | --- |
| ท.ป.4/2528 | `fc34cd223117287b961d06f48d0115748b8dd7133b0fbe1d12f5517083ef2d46` |
| Revenue Code 38–64 / 50 ทวิ | `7c4b70b52f97b39e371284cdfdcab9f75b56a44b27cadd549bdf9934221e882a` |
| Announcement 62 | `7c0d649d21615722f9a749183ce124bfd21a633c36b701a97c6921d7935e8731` |
| Amendment 100 | `41c7e4a925154b2918c6a7e96d5b49c0cd668d744bdda9a73e9a84053f067c97` |
| Amendment 121 | `07a9e19baa6f599b263182268b35b25efecbb0b3ac924a4f96874a70899785e4` |
| Revenue Code VAT articles | `2ea37b8583d096e90dea6d56290bd24e1ec8eb4dbb40f8f972a73f06cff2ad56` |
| Certificate guide PDF | `01ef15fc4a1e26f9d7a08fc09d4b21548ee38b2fcc86a5831c82aeb15308e5f9` |
| Decree 799 | `796b9aff527081720587dcf5b378833eb09bac731c3cc941fea382ce18302ba3` |
| Decree 807 | `a4e0c55a2324397af4079317d2a5138fa075fcd87eb9348771742ed7ad6dab70` |
| RD announcement 18/2569 | `9a9e278d2af0c7c2497fcd9dbaaa96581a6ccbedca9360d4b6839a0ec9d7eda1` |
| RD announcement 14/2569 | `4cdc5f8aa2eafe7adeefeb8a53583f436e5a0b9f94905625c19d0b5a60dc69a2` |

`tests/integration/finance/finance-withholding.test.ts` proves independent prepare/review/pay, net bank and gross AP/GL equality, immediate copies, cumulative installments and concurrent stale-source rejection, identity amendment and actual reversal, retained export bytes, registered VAT issuance, scoped signature intake and current grant revocation. All parties, tax identifiers, manual FX quotes, signatures and bank rows in these tests are explicitly fictional engineering fixtures. Scan availability and actual provider acceptance are recorded by their separate release gates.


The monthly owner captures actual posted bank-payment dates and current retained certificate hashes into a PND3/PND53 manual review workpaper. Independent review and a third Treasury person match a distinct tax debit; immutable assessment receipts prevent duplicate remittance. A late paid source creates a supplemental edition with only its new tax due, while the original export remains unchanged. A remitted payment source cannot be cancelled until a real positive bank credit and independently reviewed manual recovery restore its original withholding liability. The tests verify original and supplemental tax totals, bank/AP-to-GL equality, concurrent one-receipt behavior, byte checksums, stale certificates and effective payer intervals. Workpapers and fictional filing receipts demonstrate the interface and accounting lifecycle; they do not represent registered filing or Revenue Department acceptance.
