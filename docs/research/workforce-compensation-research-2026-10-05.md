# Workforce skills, performance and compensation research

2026-10-05. Primary sources checked for the People evaluation implementation. This note records evidence and product decisions; the active delivery plan remains [production plan](../production-plan.md), with terminology in [CONTEXT](../../CONTEXT.md) and behavior owned by the [Next owner catalogue](../contracts/next/catalogue.md).

## Primary-source findings

| Source | Verified guidance | Access and boundary |
| --- | --- | --- |
| [CIPD: Job evaluation and market pricing](https://www.cipd.org/en/knowledge/factsheets/market-pricing-factsheet/) — 9 May 2025 | Job evaluation and market pricing combine internal role information with external information about comparable jobs. Interpretation matters, and reviews should reflect changing work and business needs. | Public introduction verified. Detailed sections require membership; no claims about their contents are made here. |
| [CIPD: Performance management](https://www.cipd.org/en/knowledge/factsheets/performance-factsheet/) — 29 January 2026 | Performance can include task, contextual and adaptive dimensions, measured through outcomes or behaviors. Measures should suit the role and organizational goals. Complex work may suit learning objectives; shared work may suit team goals. Regular feedback and evolving objectives matter. No single approach fits every organization. | Public factsheet verified. It favors continuous conversations over reliance on a yearly appraisal. It does not prescribe ERPX's numerical scale, weights, annual dates or thresholds. |
| [ILO: Promoting equity — Gender-neutral job evaluation for equal pay](https://www.ilo.org/sites/default/files/wcmsp5/groups/public/@ed_norm/@declaration/documents/publication/wcms_122372.pdf) — 2008 | The method evaluates job requirements, rather than the holder's characteristics or performance (printed p. 50, PDF p. 57). Common factors include qualifications, effort, responsibility and working conditions (printed pp. 26–29). Required skills count regardless of how they were acquired. Methods need contextual adaptation, clear levels and review for bias and consistency. Pay comparisons need a common measurement unit (printed p. 84). | Full PDF verified. Generic difficulty and responsibility scores alone do not constitute the guide's complete analytical pay-equity method. |
| [Oracle: Understanding salary terms](https://docs.oracle.com/en/cloud/saas/human-resources/faqah/understanding-salary-terms.html) | Compa-ratio expresses salary relative to the range midpoint as a percentage: 100 means salary equals midpoint. Salary range position is a different metric: its value of 50 represents midpoint. | Transcript verified. This source defines metrics; it does not provide ERPX's monthly, daily, hourly or FTE conversion policy. |

## Decisions from the user plan

The following are ERPX starter settings, not CIPD, ILO or Oracle standards. Companies must review, adjust and explicitly confirm them before opening a review cycle.

| Setting | Starter value |
| --- | --- |
| Review period | January–December within one company; evidence can be collected throughout the year |
| Skills | 1–5 with observable behavior descriptions; professional, specialist knowledge, collaboration and leadership categories |
| Performance | 1–5; 3 means expected performance; 4 or above means outperform |
| Performance weights | Outcomes 50%, quality 30%, collaboration/adaptation 20% |
| Role fit | At least 80% of required weighted skills met, plus every essential skill met |
| Evidence sufficiency | At least 80% of required weighted skills assessed with confirmed evidence |
| Comparison charts | Compa-ratio/performance references 100% and 3; skill fit/performance references 80% and 3 |
| Approval | A human confirms assessment findings; a different authorized person approves a compensation draft |

Annual snapshots are a reporting and approval boundary. They must preserve ongoing objectives, evidence and feedback rather than making a year-end score the only performance conversation. This is an implementation interpretation of the CIPD guidance above.

## Separate the questions being answered

The implementation should store distinct versioned records for job requirements, actual work during a dated role segment, demonstrated skills, performance against goals, salary position and readiness against a chosen target role.

1. **Job requirements:** What does this role require? Store job family, level, location, expected skills, essential skills, weights, difficulty, autonomy, scope and responsibility. The company defines observable descriptions and validates its factors.
2. **Actual work:** What did the employee carry during this segment? Preserve responsibilities, changes, evidence and dates separately from the role profile. A title alone cannot describe changed scope.
3. **Demonstrated skills:** Which required skills are proven by evidence? A completed task or course is a source candidate. An evaluator must state which skill and level it demonstrates.
4. **Performance:** What results and behaviors did the employee deliver against agreed expectations? High skill does not imply high performance, and an outcome score does not establish a skill level.
5. **Salary position:** Where does approved normalized base pay sit within the applicable band? A low compa-ratio requests review; it does not establish unfairness or an automatic entitlement to an increase.
6. **Target role readiness:** Does confirmed evidence meet the requirements of the selected target profile version? An employee exceeding parts of the current role must not automatically be labeled promotion-ready or overqualified.

These separations are ERPX design decisions informed by the sources, especially the ILO distinction between evaluating a job and evaluating its holder. The module does not certify that a company's complete job evaluation scheme complies with the ILO guide.

## Compensation normalization and missing inputs

ERPX's formula is `compaRatio = normalizedFullTimeBasePay / payBandMidpoint * 100`. Both numerator and denominator must describe the same currency, pay period and full-time basis. Store the approved pay record/version, comparison date, band/version, market source/date and HR-confirmed work basis used in the calculation.

For a company-confirmed monthly comparison basis, the proposed conversions are:

| Pay basis | Monthly full-time comparison amount | Inputs HR must confirm |
| --- | --- | --- |
| Monthly | Approved monthly base amount divided by FTE | FTE greater than 0; monthly amount represents the employee's actual agreed fraction |
| Daily | Approved daily base rate multiplied by standard paid days per month | Daily rate basis, paid-day convention, and treatment of contracted hours per paid day |
| Hourly | Approved hourly base rate multiplied by standard full-time paid hours per month | Full-time paid-hours convention and whether paid breaks/leave are included |

A daily/hourly **rate** is not divided by FTE a second time. FTE describes hours or days worked; the full-time conversion already supplies standard full-time hours or days. If the source instead stores an actual total amount, its explicit basis must drive the conversion. These are company policy choices requiring confirmation, not universal conversion constants. Do not assume 30 days, 22 days, 160 hours, currency conversion, or a full-time fraction from a job title.

Missing or non-positive midpoint, absent applicable band, unconfirmed work basis, mismatched currency/period, expired market data, or multiple conflicting approved pay records must produce an explicit unavailable/stale state. Preserve raw approved pay history; do not replace an unknown value with zero. An expired benchmark is not silently current, and a market figure alone must not establish a company band without company confirmation.

## Evidence, privacy and human review

The following constraints come from the authorized product plan. They are not attributed to the four research sources as access-control or AI standards.

- Apply company and team scope before constructing any comparison set, individual detail, chart, evidence list, file response or AI context. Charts use all authorized matching people, not only the visible table page.
- Remove salary and all salary-inferable values for viewers without compensation permission, including compa-ratio, salary-derived recommendations and chart coordinates. Client-side hiding is insufficient.
- Check source-object and private-file access when evidence is attached and when it is opened. A saved evidence ID does not grant access to its source.
- Recheck actor permissions and source access when an AI job runs and when its result is retrieved. AI drafts must cite existing authorized evidence IDs, list conflicts/unknowns and remain human-reviewed. Invalid citations or provider failure must leave manual work available.
- Snapshot role profile, cycle criteria, evidence and approved pay versions at decision time. Score corrections require reasons and keep prior values. Role changes split dated assessment segments.
- A confirmed assessment may create an idempotent compensation draft. Approval checks a different approver, unchanged source pay version, company policy and next payroll boundary. Closing old pay and opening new pay must be one transaction. Issued payroll keeps its historical inputs/results.

## Acceptance cases carried into implementation

| Case | Expected separate findings |
| --- | --- |
| High performance, compa-ratio below 100 | Outperform and salary review signal; no automatic increase |
| High skill fit, low performance | Skill fit remains high; performance concern remains visible |
| Essential skill below requirement | Confirmed current-role gap even if other skills make the weighted fit high |
| Less than 80% confirmed evidence coverage | Insufficient evidence; unknown skill assessments remain unknown |
| Selected target role requirements met | Target readiness only after human evidence confirmation; current-role performance remains independent |
| Missing band or unconfirmed normalization | Salary position unavailable with the missing input shown |
| Viewer without compensation permission | No pay, band-derived salary inference, compa-ratio or salary recommendation in response data |
| Permission changes during AI processing | Access checked again; unauthorized results and evidence are withheld |
| Duplicate draft, self-approval, stale pay version | Replay returns the existing draft; self-approval/stale approval rejected |

This note records research and acceptance intent. Actual automated, browser and availability results belong in the implementation verification record after those checks run.
