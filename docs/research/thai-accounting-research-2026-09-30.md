# Thai accounting and statutory filing research — 2026-09-30

This note verifies the official sources behind the Thai accounting implementation plan. It supports [the active production plan](../production-plan.md) and [the Finance contract](../contracts/next/finance.md). It records legal and format evidence, not ERPX runtime readiness. No integration tests, authenticated accounting browser flows, official submissions or accountant acceptance were performed by this research task.

Sources were retrieved on **2026-09-30**. Buddhist Era dates below are accompanied by Gregorian dates where an implementation boundary depends on them. The official source controls; rule approval must retain its effective dates and the company's applicable facts.

## NPAEs scope and financial statements

The TFAC catalogue identifies **TFRS for NPAEs (revised 2565)** for accounting periods starting on or after **1 January 2566 / 2023-01-01**. The catalogue also links later explanatory material; explanatory material should not be mistaken for a replacement standard. [TFAC NPAEs catalogue](https://acpro-std.tfac.or.th/standard/2/-NPAEs)

The standard's paragraphs **2.1–2.4** define eligibility through public accountability. Exclusions include publicly traded securities or preparations for public issuance, a principal business holding assets for a broad group of outsiders, public limited companies and other designated entities. “SME” or an industry label alone does not establish eligibility.

Paragraphs **4.1–4.3** require financial position, profit or loss (or comprehensive income), changes in equity and notes, with comparable prior-period information. Cash flow is optional; an entity preparing it follows TAS 7. Paragraph **15.1** uses current income tax, net of withholding credits; paragraph 15.2 permits deferred tax subject to TAS 12. Paragraph **21.18** permits a non-THB functional currency under TAS 21. Therefore the planned THB profile is a product policy, not a universal prohibition on other currencies.

Paragraphs **5.18–5.23** address prior-period error correction and disclosures. Preserving issued versions and an adjustment trail must coexist with explicit restated comparatives; a stable historical report must not silently prevent a required restatement. [Official NPAEs standard, chapters 2, 4, 5, 15 and 21](https://acpro-std.tfac.or.th/test_std/uploads/files/%E0%B8%A1%E0%B8%B2%E0%B8%95%E0%B8%A3%E0%B8%90%E0%B8%B2%E0%B8%99%20NPAEs/TFRS%20for%20NPAEs_Revise%202565_%E0%B8%89%E0%B8%9A%E0%B8%B1%E0%B8%9A%E0%B8%AD%E0%B8%B1%E0%B8%9E%E0%B9%80%E0%B8%A7%E0%B9%87%E0%B8%9B%281%29.pdf)

**Implementation consequence:** company setup needs an explicit eligibility assessment, approved accounting policies, fiscal year, statutory ledger and financial-statement mapping. Moving average, straight-line depreciation and THB are proposed defaults that require company policy approval.

## VAT rate and event dates

**Decree 807 (2569)** was published on **23 August 2569 / 2026-08-23** and takes effect on **1 October 2569 / 2026-10-01**. Its amendment of the earlier decree's section 4 extends the Revenue Code rate of **6.3%** through **30 September 2570 / 2027-09-30**. The PDF's extracted Thai glyphs are corrupted, so pages 1–2 were rendered and visually read. [Official decree 807, sections 2–3](https://www.rd.go.th/fileadmin/user_upload/kormor/newlaw/dc807.pdf)

RD describes the combined VAT rate as **7%** through **30 September 2570**. Do not implement 6.3% as the total invoice rate. [RD press release 19/2569](https://www.rd.go.th/fileadmin/user_upload/news/2569thai/news19_2569.pdf)

For ordinary goods, section **78(1)** uses delivery, with earlier ownership transfer, receipt of price or tax-invoice issue triggering liability to the extent concerned. For ordinary services, section **78/1(1)** uses receipt of payment, with earlier tax-invoice issue or service use triggering liability to the extent concerned. Special transaction categories have their own provisions. [Revenue Code sections 78 and 78/1](https://www.rd.go.th/5205.html)

Order **ป.73/2541**, as amended, treats consideration called an advance, deposit, reservation or security payment according to the underlying goods/services and contains specific exceptions. Its section 3 includes qualifying refundable container deposits and specified real-estate rental security deposits. Naming money a “deposit” does not itself establish VAT treatment. [Official order ป.73/2541, sections 2–3](https://www.rd.go.th/3580.html)

Sections **80/1 and 81** separately address zero rating and exemption. Sections **82/5 and 82/6** distinguish prohibited input tax and allocation for inputs used in both taxable and non-taxable activities. Sections **82/9–82/10** link VAT adjustments to debit/credit notes and the month of their issue. [Revenue Code sections 80–82](https://www.rd.go.th/5206.html)

**Implementation consequence:** keep document date, accounting recognition date, VAT point, input-tax claim month, payment date and filing date separately. An approved rule needs transaction kind, VAT category, inclusive/exclusive pricing, effective interval, legal source and exception reason. Tests must cover partial advances and later credit notes without rewriting earlier event amounts.

## Thai documents and withholding

Section **86/4** specifies full invoice content: document label, issuer identity/address/tax number, customer identity/address, sequence, item descriptions/quantities/values, separate VAT and issue date, plus prescribed additional text. Thai language and currency are the default, subject to permitted exceptions. Sections **86/6–86/7** restrict abbreviated invoices to qualifying retail activity or approved cases. Sections **86/9–86/12** distinguish debit/credit notes and replacements; a credit note includes the original invoice reference, corrected values, VAT difference and reason. [Revenue Code sections 86/4–86/12](https://www.rd.go.th/5208.html)

Order **ป.86/2542 section 25** describes cancellation and reissue for materially incorrect invoices: retain the cancelled original, issue a new sequence number with the original invoice date, reference the original and annotate the sales-tax report. Reissuing an incorrect invoice and correcting a commercial value through a credit/debit note are distinct operations. [Official order ป.86/2542, section 25](https://www.rd.go.th/3568.html)

Withholding order **ท.ป.4/2528** differentiates payer, recipient and income category and contains time-bounded amendments. A universal service rate or a single “WHT percent” field is insufficient to choose a legally applicable rate. [Official consolidated withholding order](https://www.rd.go.th/3479.html)

Section **50 ทวิ** requires matching certificate copies and specified timing. For section 3 เตรส payments, issuance is immediate on each withholding; employment withholding has different annual/departure timing. Section 54 and foreign payments require their own treatment. [Revenue Code sections 50 ทวิ and 54](https://www.rd.go.th/5937.html)

**Implementation consequence:** retain party/branch data as issued, source payment and income category, rate-rule version, certificate status, document sequence and correction links. A payment's gross liability, net cash and withholding amounts need separate reconciliation. Legal document acceptance must include Thai PDF rendering and accountant review of the exact fields for each enabled document profile.

## Online filing deadlines

The online extension covers **statutory due dates from 2024-02-01 through 2027-01-31**, rather than transaction dates. Its eight-day extension has eligibility conditions for normal/additional filings and mixed channels. [Consolidated Ministry of Finance announcement 7, as amended](https://www.rd.go.th/fileadmin/user_upload/kormor/newlaw/mofexpandfornet7A.pdf)

| Form | Ordinary deadline described in that announcement | Qualifying online deadline before holiday handling |
| --- | --- | --- |
| ภ.ง.ด.1/2/3/53 | 7 days after payment month-end | 15 days after month-end |
| ภ.ง.ด.54 | 7 days after relevant month-end | 15 days after month-end |
| ภ.พ.30 | 15th of next month | 23rd of next month |
| ภ.พ.36 | 7 days after relevant month-end, depending on the statutory event | 15 days after relevant month-end |
| ภ.ง.ด.50 | 150 days after year-end | 158 days after year-end |
| ภ.ง.ด.51 | 2 months after the first six-month period | 2 months and 8 days |

Amendment **9**, effective **17 December 2568 / 2025-12-17**, changes the qualifying online deadline for **ภ.ง.ด.1ก and 1กพิเศษ** to **8 March** of the following year. The supplied consolidated announcement includes this amendment. [Official amendment 9](https://www.rd.go.th/fileadmin/user_upload/kormor/newlaw/mofexpandfornet9A.pdf)

**Implementation consequence:** retain statutory deadline, channel, eligibility, holiday adjustments and approved deadline; verify official calendars and special extensions. No general extension after **2027-01-31** was verified. Require a new source review after that boundary.

## RD export formats and the official validation boundary

RD publishes **Format กลาง V2.0** for **ภ.ง.ด.1/1ก/1กพิเศษ, 2/2ก, 3/3ก and 53**, dated **16 June 2025**. It identifies the SWC or SWC-UI route. The same page separately lists older Version 1 payroll/GFMIS conversion formats. These are different formats and routes. [RD official format index](https://www.rd.go.th/63724.html)

The V2.0 PND1 family spec requires a form-specific filename, one header followed by detail records, Unicode UTF8, pipe-separated fields without leading/trailing pipes and CR/LF records. Field requirements vary by submission route; names, branch identifiers, year/month and submission type must follow its tables. [Official PND1/1ก V2.0 specification, pages 1–4](https://www.rd.go.th/fileadmin/user_upload/WHT/Download/FormatPND1V2_0.pdf)

PND3 and PND53 have separate record tables, mandatory fields and filenames; their specifications also describe UTF8, pipe fields and CR/LF. An ERPX CSV with similar amounts does not establish conformance. [Official PND3 V2.0 specification](https://www.rd.go.th/fileadmin/user_upload/WHT/Download/FormatPND3V2_0.pdf), [official PND53 V2.0 specification](https://www.rd.go.th/fileadmin/user_upload/WHT/Download/FormatPND53V2_0.pdf)

RD's SWC documentation describes transmission from the taxpayer's computer/server to RD. The manual page lists **SWC-UI 1.5.4**, dated **21 September 2026**. Its WHT notice distinguishes the **`.whtc.rdx`** generated through SWC for the stated agent-filing route from input data supplied to the component. [SWC service description](https://www.rd.go.th/66725.html), [current listed SWC-UI manual](https://www.rd.go.th/63739.html), [RD WHT notice and filing route](https://www.rd.go.th/62977.html)

**Validation not performed:** no ERPX export was passed through SWC/SWC-UI, no RD-issued validation/receipt was obtained, and no taxpayer registration or filing credentials were used. “Officially validated” must remain unavailable until the exact export, tool version, result and reviewer are retained. Local format tests alone cannot establish this status.

For **ภ.พ.30/36, ภ.ง.ด.54 and ภ.ง.ด.50/51**, the plan's first implementation boundary remains reconciled figures/workpapers for a person to file, with additional returns and filing/payment evidence retained. This research establishes no new direct-filing connector for those forms.

## Year-end tax and DBD preparation

Revenue Code **65, 65 ทวิ and 65 ตรี** distinguish taxable profit and allowable/non-allowable expenses. Section **65 ตรี(12)** addresses qualifying losses carried from no more than five years before the current accounting year. Section **67 ทวิ** governs the half-year return, while **68** governs the annual return. These require separate tax workpapers rather than treating accounting profit as taxable profit. [Official Revenue Code sections 65–68](https://www.rd.go.th/5939.html)

The RD explanation of section **65 ทวิ** illustrates statutory depreciation limits, daily apportionment and residual-value rules. Tax depreciation cannot automatically equal the approved accounting depreciation schedule. [RD conditions for taxable-profit calculation](https://www.rd.go.th/828.html)

The supplied **DBD filing announcement (2568)** requires the specified entities to submit financial statements and audit reports through DBD e-Filing within the legal period; its attached ส.บช.3 asks for meeting approval and auditor information. The scanned PDF was rendered and inspected. [Official DBD filing announcement and ส.บช.3](https://www.dbd.go.th/data-storage/attachment/ebacbc3fea6376dea0881a8b.pdf)

A publicly downloadable DBD guide was successfully retrieved in this research. It describes **DBD XBRL in Excel V2.0**, selecting the legal entity, business and accounting-standard taxonomy, preparing an Excel file with the corresponding Java builder, validation and submission. It is stronger evidence than only knowing that e-Filing exists. It does **not** prove which exact taxonomy/template applies to an ERPX company and period or that an ERPX-generated file passes the current official builder/importer. [DBD download and preparation guide, pages 7–16 and 36 onward](https://efiling.dbd.go.th/efiling-documents/01_ManualFN.pdf)

**Implementation boundary:** export a mapped preparation workbook and Thai PDF, label them as preparation artifacts, and retain the company/period mapping, auditor adjustments, approval date and issued/filed statement versions. Download the applicable official template and retain official validation before claiming an import-ready DBD package. No DBD template import or submission was performed here.

DBD's business-entity guide states that accounting books and supporting documents must generally be retained for at least **five years from closing**, with location and loss-reporting requirements. This is a minimum supported by that guide, not an automatic destruction date covering every tax rule or hold. [DBD duties of business entities, printed pages 7 and 18](https://www.dbd.go.th/data-storage/attachment/f183f131d7ca2105131b32fef.pdf)

## Evidence required before a flow is accepted

The following are implementation and acceptance requirements from the user's plan, not statutory facts newly established by browsing:

- A blank eligible company can complete statutory-ledger setup, opening balances and reconciliation with approved policies.
- Posted financial events and correction references reproduce dated AR/AP and GL reports, while explicit restatement versions remain traceable.
- Transaction-derived VAT/WHT registers reconcile to GL; issued documents preserve their identity and correction history.
- Batch posting enforces 100 drafts/1,000 lines, scope, versions, dependencies, open periods, independent review and all-or-nothing transactions.
- Cash results, accounting, tax events, approval evidence and idempotency receipts commit together; exceptions preserve the actual bank result.
- Each business source has approved expected entries; stock/WIP, payroll, assets and subsidiary ledgers reconcile to GL without duplicate cost recognition.
- Two consecutive monthly closes and a year-end rehearsal retain independent review, complete NPAEs statements, tax workpapers, auditor evidence and filing/payment receipts.
- Integration, actual authenticated development browser flows, Thai document rendering, typecheck/lint and the isolated build pass before changing the Finance readiness state. Official export-tool checks and accountant acceptance must be recorded separately from automated checks.

This note changes no readiness status. Remaining official-tool validation, company eligibility, accountant-reviewed expected outcomes and actual filing evidence are explicit acceptance work.
