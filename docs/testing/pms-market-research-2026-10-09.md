# PMS market research and ERPX comparison method

2026-10-09. Primary sources checked for the ERPX hotel PMS plan. This note records vendor evidence, research limits and the user's product decisions. The active delivery plan remains [production plan](../production-plan.md); current behavior belongs to the [PMS contract](../contracts/next/pms.md), with module ownership in [ADR-040](../adr/ADR-040-hospitality-module-ownership-and-guest-accounts.md). Vendor claims below establish what the vendor documents, rather than independent performance measurements or purchased entitlements.

## Product decisions and reference recommendation

- Build ERPX for multiple properties with **30–200 rooms per property**, with a phased real-hotel pilot. Compare vendors to choose capabilities, UX references and future providers; this research does not select or purchase a replacement PMS.
- Cover full hotel operations and an ecosystem integration roadmap. Do not implement every enterprise MICE, spa, loyalty, door-lock or revenue-management product internally. Record each capability as **ERPX-owned**, **external integration**, or **deferred**, with evidence and a delivery phase.
- Start with **direct bookings and manually recorded OTA bookings**. Clearly identify their source and external reference. Manual recording is not channel synchronization and does not prevent another channel from selling its own remaining inventory.
- Start government-facing work with internal documents, required guest-data capture and reviewable exports. Live government submissions, e-Tax delivery and undocumented automated APIs are deferred. A prepared export is not evidence that an authority accepted it.
- Provide clickable staff and guest mockups with generated **Standard, Deluxe and Suite** room images and three UI variants. Images and guest records are demonstration data. Do not present generated rooms as photographs of an actual property.

**Recommended reference combination:** Cloudbeds' current calendar and reservation panel for routine front-desk work; OPERA's group/Guest Account/charge-routing workflows for operational and financial depth; Mews' timeline, self-service and automation concepts for guest journeys. Use Comanche to compare Thai hotel back-office expectations, and SiteMinder to compare distribution controls. This is an ERPX design recommendation inferred from the documented capabilities below, not a claim that one vendor combines every capability in one package.

**Future OTA partner candidate:** evaluate SiteMinder pmsXchange first when automated distribution enters an authorized phase. It is documented as a two-way PMS/RMS integration for availability, rates, restrictions and reservations. It remains a candidate pending fit, credentials, certification, pricing and a signed commercial decision. The current pilot default remains direct/manual OTA. [SiteMinder developer guide](https://www.siteminder.com/developer-guide/)

## Six-vendor comparison

“Fit” is an ERPX assessment based on workflow breadth, not a vendor-certified room-count limit. “Native” does not mean included in every subscription. Confirm the specific product version, package, country and contract before marking a capability available.

| Product | Documented capability families | Package, add-on and integration boundaries | Relevance to ERPX / assessment of fit |
| --- | --- | --- | --- |
| **Oracle OPERA Cloud** | Reservations, rates/inventory, guest/company/agent profiles, group blocks and rooming lists, configurable check-in/e-sign, multiple Guest Accounts and charge routing/transfers, reports, mobile room operations. | Full F&B through MICROS Simphony; sales/banquets through Sales and Event Management; distribution, loyalty, vacation ownership and Nor1 upsell have separate product/entitlement considerations. | Reference for full-service hotels, resorts, group billing and chains; useful financial benchmark across the target 30–200-room range. [Official product and gallery](https://www.oracle.com/hospitality/hotel-property-management/hotel-pms-software/) |
| **Mews** | PMS, reservation timeline, group blocks/pickup/release, billing, payments, booking engine, RMS, channel management, guest technology and BI. | Current public pricing uses **Core / Pro / Enterprise**. Pro adds depth such as Flexkeeping housekeeping/maintenance, guest self-service and advanced BI. POS and bookable spaces are add-ons; payment fees are separate. Older plan materials must not define a current quote. | Reference for automation and guest journeys; provider fit in Thailand remains conditional on language, payments and localization proof. [PMS](https://www.mews.com/en/property-management-system), [Reservation workflows](https://www.mews.com/en/products/reservation-management), [Current pricing](https://www.mews.com/en/pricing) |
| **Cloudbeds** | PMS/calendar, reservations/groups, housekeeping, payments, reporting and multi-property operation. | **Flex** supports external distribution; **One** includes its channel manager and booking engine; **Experience** adds guest experience/reputation. Enterprise is custom. Revenue Intelligence is an add-on; POS is provided through marketplace partners. | Strong operational/UX reference for independent hotels and growing groups; documented Thai support areas below make it useful for local comparison. [PMS](https://www.cloudbeds.com/property-management-system/), [Current plans](https://www.cloudbeds.com/pricing/) |
| **SiteMinder** | Channel management, OTA distribution, booking engine, metasearch/GDS, business intelligence, pricing insights, payments, websites, guest engagement and centralized distribution. | **A distribution/commerce companion to an existing PMS**, not the full front-office PMS in this comparison. SiteMinder, Plus and Groups & Chains have different entitlements. | Distribution-grid and integration reference; future ERPX channel partner candidate, not a prerequisite for the manual pilot. [PMS integrations](https://www.siteminder.com/pms-property-management-system/), [Channel manager](https://www.siteminder.com/channel-manager/), [Plans](https://www.siteminder.com/pricing/) |
| **Little Hotelier** | Central booking calendar, room assignment, check-in/out, invoices/payments, housekeeping status, guest emails, mobile app, channel manager, direct booking and pricing insights. | Thai pricing page currently shows **Flex / Pro**; payments, websites, metasearch and guest capabilities depend on plan/market. Support distinguishes new Platform and Classic versions. | Simplicity/mobile benchmark at the lower edge of ERPX's target; official positioning focuses on **1–30 rooms**, so do not assume enterprise workflow depth at 100–200 rooms. [Thai product](https://www.littlehotelier.com/th/property-management-system/), [Thai plans](https://www.littlehotelier.com/th/pricing/), [Version distinction](https://www.littlehotelier.com/contact/) |
| **Comanche Gogoji** | Current homepage identifies FOS, POS, accounting, inventory, interfaces and management reporting, covering hotel, residence, group and restaurant operations. | Current API access, deployment architecture, package/module details, TM30, e-Tax and payment methods need vendor documentation/demo. The homepage's broad interface claims do not prove a particular integration. | Local reference/shortlist for Thai hotel F&B and back-office expectations. The vendor states a customer range from 7 to over 4,000 rooms and front-office support around the clock. [Official site](https://www.gogoji.co.th/) |

### Operational capabilities to exercise, rather than count

Use these as ERPX comparison groups, not as a claim that every vendor/package supplies them natively:

| Family | Evidence to compare | ERPX build boundary |
| --- | --- | --- |
| Inventory and reservations | Physical units versus room types; available dates; room assignment; holds/blocks; groups and rooming lists; cancellation/no-show; moves/split stays. | Own the hotel inventory/reservation model and transaction correctness. |
| Front desk and guest records | Arrivals/departures; check-in/out; identity data; guest preferences; extensions; desk handover and mobile operation. | Own the staff workflow and authorized guest record access. |
| Hotel operations | Clean/dirty/inspected states; housekeeping assignment; maintenance/out-of-order; minibar and service requests. | Own core coordination; integrate specialist products where justified. |
| Hotel finance | Deposits/payments; Guest Accounts; split/routed charges; reversals/refunds; corporate balances; cashier shifts/night audit; Thai documents and ledger reconciliation. | Reuse ERPX accounting/POS ownership and prove reconciled results. |
| Distribution and guest journey | Rate/restriction grids; channel mapping; direct booking; secure guest access; messaging; upsells and payment links. | Direct/manual first; provider adapters and advanced automation later. |
| Property portfolio and ecosystem | Property scope; role permissions; audit; portfolio reporting; POS/accounting; keys, RMS, MICE, spa and loyalty. | Own tenancy/control boundaries; explicitly phase integrations and deferred capabilities. |

The workflow groups are informed by the [OPERA gallery](https://www.oracle.com/hospitality/hotel-property-management/hotel-pms-software/), [Mews reservations](https://www.mews.com/en/products/reservation-management), [Cloudbeds housekeeping guide](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/25695101078427-Housekeeping-Everything-you-need-to-know), and [Cloudbeds night-audit guide](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/31030047512347-Enhancements-to-the-Night-Audit-Process-in-Cloudbeds-PMS). These sources describe product behavior, not ERPX acceptance results.

## Thailand: verified support and remaining unknowns

| Area | Evidence checked | What must not be inferred |
| --- | --- | --- |
| Cloudbeds language | Official matrix lists Thai for the PMS UI and knowledge-base articles, with gaps for activity-log language, direct support/onboarding, university subtitles and billing portal. Correct article ID is **1260804304509**. [Language matrix](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/1260804304509-Cloudbeds-PMS-supported-languages) | Thai UI does not establish Thai support for every service. |
| Cloudbeds local assistance | Official consultant directory lists Thai-speaking partners and Thailand-based implementation/optimization services. [Certified consultants](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/40687180725787-Cloudbeds-Certified-Consultant-Partner-Network) | Partner assistance has separate scope/commercial terms; it is not evidence of native Thai support 24/7. |
| Cloudbeds payments | Thailand appears in official Cloudbeds Payments availability. The partner directory also lists Thai-region options including KBANK, KTB, BBL, BAY and Omise; workflows and group-account support differ. [Payments availability](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/360057382053-Cloudbeds-Payments-Everything-you-need-to-know), [Payment apps](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/10753531773083-Payments-Apps) | Merchant approval, terminals, payment methods and native PromptPay support must be confirmed individually. Country availability alone is insufficient. |
| Cloudbeds TM30 | Mecx-Tech's listed integration creates an XLS of checked-in foreign guests, sends it to the property the next morning and requires manual upload. Cloudbeds also documents guest-information collection templates. [Government apps](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/7319407151515-Government-Integration-Apps), [Guest-data templates](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/38046227239835-Use-Guest-Information-Templates-for-Government-Data-Compliance) | This is not automatic submission or proof of acceptance by Immigration. |
| Cloudbeds fiscal language | Thai appears among supported invoice, pro-forma and credit-note template languages. [Fiscal document languages](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/45699206239643-Generating-Fiscal-Documents-in-Different-Languages) | Translation does not prove current Thai tax-invoice/e-Tax compliance. |
| Mews localization | Official language table supports Unicode data entry but does not list Thai UI labels. POS availability page inspected does not list Thailand. [Language support](https://help.mews.com/s/article/language-support-in-mews?language=en_US), [POS countries](https://help.mews.com/s/article/where-can-i-get-mews-pos) | Thai UI, local POS/payments, tax documents and TM30 remain unverified; absence from these pages is not proof of all-product unavailability. |
| OPERA Thailand evidence | Historical official docs include Thailand payment-tax templates and country-specific tax-report changes. [21.4 stationery templates](https://docs.oracle.com/en/industries/hospitality/opera-cloud/21.4/ocsuh/c_reports_stationery_reports_group.htm), [21.4 release contents](https://docs.oracle.com/en/industries/hospitality/opera-cloud/21.4/oprnc/toc.htm) | Historical capability and global-compliance claims do not certify the current Thai configuration, e-Tax, TM30 or gateway. |
| Little Hotelier local support | Official Thai-property case reports a Thai support department. Current contact page lists Thai phone hours Monday–Friday 08:00–17:00 GMT+7 and round-the-clock English support. [Thai customer case](https://www.littlehotelier.com/testimonials/little-hotelier-delivers-successful-start-to-mamori-house-at-koh-larn/), [Support hours](https://www.littlehotelier.com/contact/) | Do not advertise Thai-language phone support 24/7 or infer Thai tax/TM30 functionality. |
| Comanche / SiteMinder local features | Comanche identifies its Bangkok office/support; SiteMinder publishes a Thai site. [Comanche](https://www.gogoji.co.th/), [SiteMinder Thai site](https://www.siteminder.com/th/) | TM30, e-Tax, native PromptPay and exact Thai tax-document compatibility were not sufficiently verified in this research. |

**Government baseline:** a cached official Immigration search result describes individual online entry and Excel import for multiple foreign guests. The [TM30 manual URL](https://tm30.immigration.go.th/TM30/Foreigner/TM30TH/Notify-Residence.html) was not retrievable during direct rechecks: the root research open returned 403 and this note's follow-up could not retrieve the manual. Its current availability and precise current spreadsheet specification remain unverified. Before implementing a TM30 export, obtain and validate the current official template through the hotel's authorized process. Do not promise an automatic API from this cached evidence.

The Revenue Department describes authorized service providers for preparing/submitting electronic information and distinguishes e-Tax service categories. This supports evaluating a registered provider for a later e-Tax integration; it does not document unrestricted ERPX submission access. [Revenue Department service-provider overview](https://efiling.rd.go.th/ef-cms-web/service-provider)

## Integration access and reliability constraints

| Provider | Verified constraints | Planning consequence |
| --- | --- | --- |
| SiteMinder pmsXchange | OTA XML flows use SOAP 1.1; selected operations use REST/JSON. HTTPS domain endpoint on port 443, PMS-level PUSH/PULL choice, persistent retries, duplicate prevention and delta updates are documented. Changes should reach SiteMinder within two minutes. Partner test access and certification are required. | Do not design the adapter as REST-only. Budget mapping, protocol validation, durable inbox/outbox, acknowledgements, retries and certification; do not expose it as active in the manual pilot. [Requirements](https://developer.siteminder.com/pmsxchange-api/guides/integration-requirements), [Testing/certification](https://developer.siteminder.com/pmsxchange-api/guides/testing-and-certification) |
| Mews Connector | Partner journey includes development/test, certification and a production pilot before marketplace publication. Production Client Token and per-property Access Token are distinct. Booking Engine API follows an exception to certification/pilot. | Select the API appropriate to the use case; an open documentation page is not a production customer connection. [Partner journey](https://docs.mews.com/getting-started/your-journey) |
| Cloudbeds API | Own-property access can be included or a paid add-on depending on package. Commercial technology partners follow partnership/agreement, test account, scopes and certification steps. | Separate custom hotel access from a commercial ERPX integration. Confirm scopes and production credentials before scheduling a live dependency. [Property/group access](https://developers.cloudbeds.com/v1.2/docs/property-and-group-account-api-access), [Integration guide](https://developers.cloudbeds.com/docs/integration-guide) |
| Oracle OHIP | Paid usage includes sandbox. Partner production access requires appropriate OPN/Marketplace listing and hotel approval; application-key owner bears API costs. The current FAQ gives approximate overage pricing of USD 20 per 10,000 calls, separate from the PMS subscription. | Include development/event/API consumption in TCO and verify the applicable commercial offer. Do not use an OCI free-tier assumption for OHIP. [Current OHIP FAQ](https://docs.oracle.com/en/industries/hospitality/integration-platform/ohipu/c_faqs.htm) |
| Comanche | Current official site claims interfaces but public access/certification terms were not found in the sources inspected. | Treat specific interfaces as unverified vendor dependencies until written documentation and a sandbox/demo prove them. [Official site](https://www.gogoji.co.th/) |

No account signup, sales form, vendor outreach, production credential request or commercial commitment was made for this research.

## Published pricing versus quote requirements

| Product | Public evidence as of 2026-10-09 | Comparison status |
| --- | --- | --- |
| OPERA Cloud | Product page directs to demo/sales; no comparable Thai PMS tariff found. [Product](https://www.oracle.com/hospitality/hotel-property-management/hotel-pms-software/) | Quote required; collect PMS, modules, implementation and OHIP separately. |
| Mews | Per-room model and tailored quote; payments charged separately; POS and bookable services are add-ons. [Pricing FAQ](https://www.mews.com/en/pricing) | Quote required for the same property/country scenario. |
| Cloudbeds | Flex, One, Experience and Enterprise use “Request a quote”. Native booking engine/channel manager have no added booking commission; metasearch and other charges require separate inspection. [Pricing](https://www.cloudbeds.com/pricing/) | Quote required; use the correct distribution/package combination. |
| SiteMinder | Room-count pricing; inspected rendering returned placeholder amounts. Groups & Chains directs to sales. Transactional fees apply in available markets. [Pricing](https://www.siteminder.com/pricing/) | Regional quote required; do not turn placeholders into estimated numbers. |
| Little Hotelier | Thai homepage shows AUD 39/month + 1%, while Thai pricing renders Flex/Pro with incomplete amounts. Pricing FAQ defines the booking-fee basis and excludes taxes/extras. [Thai homepage](https://www.littlehotelier.com/th/), [Thai pricing terms](https://www.littlehotelier.com/th/pricing/) | Public indicative observation only; verify the current plan, regional currency, room limit and fee base. Do not extrapolate this number to 30–200 rooms. |
| Comanche Gogoji | No public tariff found on current official homepage. [Official site](https://www.gogoji.co.th/) | Quote required, with module/support/deployment details. |

Unknown costs remain **unknown**, not zero. Room numbers below are comparison inputs chosen for ERPX; no vendor prices or property revenue forecasts have been invented.

### Three-year TCO formula and inputs

Use one currency and one comparison date for each scenario. Store quote currency, FX source/date, quote validity, tax recoverability, minimums, annual escalation and contractual term. Do not assume straight per-room scaling when a package has minimums or bundled prices.

```text
N = rooms per property × properties

TCO_36 = setup_and_migration
       + implementation_and_training
       + initial_hardware
       + sum(monthly_license_and_modules[month], month = 1..36)
       + sum(monthly_integrations_and_API[month], month = 1..36)
       + sum(monthly_runtime_and_operations[month], month = 1..36)
       + sum(transaction_and_distribution_costs[month], month = 1..36)
       + hardware_replacement_and_maintenance
       + internal_labor_and_change_costs
       + nonrecoverable_taxes
       + exit_and_export_costs

Net_comparison_36 = TCO_36 - separately_evidenced_avoided_existing_costs
```

For the build option, include ERPX engineering, design, verification, security maintenance, provider certification and support work. Do not count shared ERPX infrastructure twice; disclose allocation. Keep gross TCO and claimed avoided costs separate rather than crediting hypothetical efficiency savings automatically.

| Cost category | Inputs required; no default monetary value | Classification / double-counting rule |
| --- | --- | --- |
| PMS licence/modules | Group/property/room/unit fee, minimum package, discount, relevant booking/housekeeping/BI/guest/RMS/POS modules, increases. | Fixed/per-property/per-room recurring; use the contracted bundle once. |
| Implementation | Setup, migration/cleanup, mappings, opening balances, templates, training, travel, go-live coverage, parallel-run costs. | One-time plus any planned retraining; vendor and internal labor separate. |
| Integrations/API | Channel-manager subscription, connector fee, development/sandbox usage, events/calls, certification, RMS/accounting/key/payment/e-Tax provider fees. | Recurring/usage/one-time; separate adapter build from vendor access charges. |
| Payments | Processed value by method/country, MDR, per-transaction fee, tokenization, terminal rental, preauthorization/capture, payout, FX/DCC, refunds and chargebacks. | Variable plus fixed; do not bill a deposit and its settlement twice in revenue assumptions. |
| Distribution | OTA/wholesaler commissions, reservation-delivery fees, metasearch/ad spend, direct-booking fees and fee bases. | Show external channel commission separately from PMS-added fees; common OTA costs remain visible in every option. |
| Hardware/network | Readers, payment terminals, tablets, printers, key encoders, delivery/installation, warranty/replacement, connectivity. | Purchase or rental, not both for the same device; include shared allocation. |
| Runtime/operations | Hosting, database, storage, backups/restore checks, monitoring, connectivity, support/on-call, licence administration. | Recurring; disclose any shared ERPX allocation. |
| Internal work/change | Build/maintenance effort, training time, reconciliation, manual OTA workload, defect/incident handling, measured downtime and adoption work. | Hours × confirmed loaded labor rate; distinguish measured from forecast. |
| Tax/exit | Nonrecoverable taxes, data export/retrieval, retention, transition training, replacement connector/migration and termination fees. | Record refundability/recoverability; do not add recoverable VAT as permanent cost. |

Use hotel-confirmed occupancy, ADR, channel mix, booking count, payment value/mix and refund/dispute history to estimate variable costs. If inputs are missing, publish fixed-cost comparison plus an explicit unpriced variable-cost list. Provide low/base/high cases only with disclosed inputs and sources.

| Scenario | Rooms/property | Properties | Total rooms | One-time costs | 36-month fixed costs | 36-month variable costs | Labor/tax/exit | TCO_36 |
| --- | ---: | ---: | ---: | --- | --- | --- | --- | --- |
| S1 | 30 | 1 | 30 | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ยังคำนวณไม่ได้ |
| S2 | 30 | 3 | 90 | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ยังคำนวณไม่ได้ |
| S3 | 100 | 1 | 100 | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ยังคำนวณไม่ได้ |
| S4 | 100 | 3 | 300 | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ยังคำนวณไม่ได้ |
| S5 | 200 | 1 | 200 | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ยังคำนวณไม่ได้ |
| S6 | 200 | 3 | 600 | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ยังคำนวณไม่ได้ |

Complete each scenario for **ERPX build**, **vendor PMS alternative**, and **future ERPX + provider integration** with the same hotel assumptions. This is a reusable comparison template; no quote requests were sent.

### Product-by-scenario cost comparison

This table applies the six scenarios above to every benchmark. Each cell is the **complete three-year total**, including PMS, sales channels, payment processing, migration, training, support, API and data exit. Inspection date: **2026-10-09**. Published indicative pricing above does not establish a complete comparable total. A missing quote remains unknown; it is not a zero-cost option.

| Option | S1: 30 rooms × 1 | S2: 30 rooms × 3 | S3: 100 rooms × 1 | S4: 100 rooms × 3 | S5: 200 rooms × 1 | S6: 200 rooms × 3 |
| --- | --- | --- | --- | --- | --- | --- |
| Oracle OPERA Cloud | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา |
| Cloudbeds | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา |
| Mews | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา |
| Comanche Gogoji | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา |
| Little Hotelier | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา / ตรวจขนาดที่รองรับ | ต้องขอใบเสนอราคา / ตรวจขนาดที่รองรับ | ต้องขอใบเสนอราคา / ตรวจขนาดที่รองรับ | ต้องขอใบเสนอราคา / ตรวจขนาดที่รองรับ |
| SiteMinder with an independently priced PMS | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา | ต้องขอใบเสนอราคา |
| ERPX build | ยังไม่ทราบต้นทุนแรงงานและดำเนินงาน | ยังไม่ทราบต้นทุนแรงงานและดำเนินงาน | ยังไม่ทราบต้นทุนแรงงานและดำเนินงาน | ยังไม่ทราบต้นทุนแรงงานและดำเนินงาน | ยังไม่ทราบต้นทุนแรงงานและดำเนินงาน | ยังไม่ทราบต้นทุนแรงงานและดำเนินงาน |
| ERPX plus SiteMinder after pilot | ต้นทุน ERPX + ต้องขอใบเสนอราคา | ต้นทุน ERPX + ต้องขอใบเสนอราคา | ต้นทุน ERPX + ต้องขอใบเสนอราคา | ต้นทุน ERPX + ต้องขอใบเสนอราคา | ต้นทุน ERPX + ต้องขอใบเสนอราคา | ต้นทุน ERPX + ต้องขอใบเสนอราคา |

Little Hotelier's stated small-property positioning does not prove that a 100/200-room property is supported. SiteMinder is the distribution/connector reference; its subscription alone cannot be compared with a complete PMS total. Use the official product/pricing sources above when filling quotations, and keep all eight required cost categories itemized under each scenario rather than inserting an assumed bundled price. Fictional room counts are authorized; fictional vendor quotations are not evidence.

## Weighted scripted demo scorecard

The weights and thresholds are ERPX comparison defaults chosen for this task, not an industry certification. Use them to choose reference patterns and future providers as well as to evaluate ERPX. Run identical scripts with front desk, housekeeping, finance and management; use synthetic records, the same room/property fixtures and the same declared capabilities.

Score each criterion 0–5: **0** absent/failed; **1** impractical manual workaround; **2** works with material limitations; **3** completes the script; **4** completes it with fewer measured errors/steps; **5** demonstrates the script, relevant failure recovery and usable evidence. Mark unseen capabilities **unverified** rather than awarding points for a brochure. Publish verified score and evidence coverage; do not silently renormalize missing criteria.

```text
Weighted score / 100 = sum(weight × observed_score / 5)
Evidence coverage % = sum(weights with completed, recorded script evidence)
```

| Criterion | Weight | Script/evidence that earns the score |
| --- | ---: | --- |
| Hotel operations and inventory | 25 | Reservation/assignment/groups, moves/split stays, housekeeping and unavailable rooms; measured completion and conflict handling. |
| Finance and Thai-document workflow | 20 | Deposits, routed/split Guest Accounts, POS charges, refunds/reversals, cashier/night audit and reconciled documents; distinguish internal output from authority acceptance. |
| Staff/guest usability | 15 | Timed tasks, Thai labels, keyboard/mobile use, clear errors, room-photo selection and guest flow; record staff and guest results separately. |
| Distribution/integration reliability | 15 | Manual source tracking now; when in scope, mapping, duplicate/reordered events, cancellation, retries, acknowledgment and reconciliation. |
| Multi-property permissions/audit | 10 | Property switching, permitted portGuest Account views, denied cross-property access and actor/reason/history evidence. |
| Support, maintainability and portability | 10 | Demonstrated troubleshooting/runbook, relevant support hours, sandbox/API access, export/migration and restore evidence. |
| Three-year cost and commercial fit | 5 | Complete comparable TCO inputs, fees/terms/exit coverage and clear unknowns; no points for an invented cheap quote. |
| **Total** | **100** | |

### Common script set

| Script | Actions and expected evidence |
| --- | --- |
| 1. Direct / manual OTA booking | Quote dates/type/rate, create a direct reservation and manually record an OTA reservation/reference; show source, availability and balance. Clearly state manual channel inventory obligations. |
| 2. Last-room concurrency | Two users attempt the same final physical-room night; preserve an explicit accepted/failed outcome and audit. A rejected booking must not leave a deposit/ledger mutation. |
| 3. Arrival and identity | Assign a clean/inspected room, collect required guest data, take a deposit and check in; show authorized fields and unavailable/missing-data guidance. |
| 4. Room move / split stay | Move an in-house guest across dates/types, preserve original history and financial effects, and avoid counting checkout day as an occupied night. |
| 5. Group/corporate split | Group block/rooming list, partial pickup/release, company-paid room charge and guest-paid extras; show correct remaining inventory and distinct liabilities. |
| 6. Housekeeping/maintenance | Checkout makes the room require cleaning; assign/complete/inspect; mark out of service and recover availability without losing the maintenance record. |
| 7. POS and payment recovery | Post a room charge, replay the request, receive a partial payment, reverse/refund with a reason; verify exactly one financial effect for each logical action. |
| 8. Checkout and close | Handle an outstanding balance according to authorized policy, produce internal documents, close cashier/business day and reconcile PMS/POS/ledger totals; prove safe replay. |
| 9. Thai output boundary | Review guest/company tax fields and document/export drafts. Demonstrate missing-data feedback and delivery status. Do not label preview/export as e-Tax/TM30 accepted. |
| 10. Scope and guest access | Staff in property A cannot inspect/mutate property B without permission; guest access reveals only its authorized reservation, charges/documents and requests. |
| 11. Future live provider recovery | In a provider sandbox when authorized: create/modify/cancel multi-room reservations; replay/ reorder messages, remove mappings, simulate outage and recover/reconcile. Until then mark live-provider capability deferred/unverified. |
| 12. Portability and service operation | Export agreed records/attachments with access checks, demonstrate backup/restore or documented evidence, identify support coverage and resolve a simulated incident. |

**Mandatory pilot gates override weighted scores:** no demonstrated cross-property/guest data leakage, double-allocation, duplicate financial posting, unreconciled issued balances, or false authority/provider-success labels. Verify required desk/housekeeping/finance tasks with real pilot roles and provide manual recovery. Provider purchase/go-live also requires country/package eligibility, usable production-access terms, support/exit details and the hotel's authorized commercial decision. A useful UI reference can still be studied when its provider fails commercial/localization gates.

## Mockup references and limits

| Reference | Usable official evidence | ERPX exploration |
| --- | --- | --- |
| OPERA | Public dashboard, reservations, blocks, check-in, billing and mobile gallery. [Gallery](https://www.oracle.com/hospitality/hotel-property-management/hotel-pms-software/), [Dashboard image](https://www.oracle.com/a/ocom/img/rc118-opera-cloud-dashboard-1.webp), [Billing image](https://www.oracle.com/a/ocom/img/rc118-opera-cloud-billing-9.webp) | Compare information density, group controls and Guest Account routing. Study behavior; build ERPX's own visuals and terminology. |
| Cloudbeds | New Calendar guide updated 2026-09-17 keeps the grid visible beside a right reservation panel with state/role-sensitive actions and balance information. [New Calendar guide](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/48384262863771-Manage-reservations-from-the-New-Calendar) | Variant A: calendar-led front desk with right drawer and focused arrival/payment tasks. |
| Cloudbeds version caveat | Classic Calendar documentation says retirement is planned on or around 2027-01-15. [Classic guide](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/235146587-Classic-calendar-Everything-you-need-to-know) | Prefer current calendar references; do not reproduce an obsolete workflow by accident. |
| Mews | Current reservation/timeline page and PMS visuals document drag moves/extensions/splits, blocks and digital guest journeys. [Reservations](https://www.mews.com/en/products/reservation-management), [PMS visuals](https://www.mews.com/en/property-management-system) | Variant C: compact work queues, mobile housekeeping and staff/guest self-service flow. |
| SiteMinder | Official certification guide includes distribution inventory and restrictions screenshots. [Certification examples](https://developer.siteminder.com/pmsxchange-api/guides/testing-and-certification) | Compare future rate/restriction/channel controls; preserve explicit inactive/manual status in the current mockup. |
| Little Hotelier | Thai PMS page plus seller-published mobile screenshots; support distinguishes Platform versus Classic. [Thai PMS](https://www.littlehotelier.com/th/property-management-system/), [App Store screenshots](https://apps.apple.com/au/app/little-hotelier/id1130899763), [Versions](https://www.littlehotelier.com/contact/) | Compare task simplicity and mobile context; record the version inspected. |
| Comanche | Current public homepage has an intro video/demo request, but detailed current public UI evidence was insufficient for a comprehensive assessment. [Official site](https://www.gogoji.co.th/) | Keep exact Thai back-office claims unverified until evidence exists; no demo request was submitted. |

**Variant B** is ERPX's room-led exploration: visual room/type cards with generated photos, an operational room board and detail drawer. Keep room state, arrivals/departures, balance and conflicts visible; a photo must not displace an operational warning. All three variants use the same Standard/Deluxe/Suite data and staff/guest scripts so the comparison measures navigation and comprehension rather than different feature sets.

For long editors, follow the workspace's form-drawer convention: fixed header, scrolling body and full-width small-screen presentation. Keep guest identity and documents private. Vendor images above are references for inspection, not assets licensed for embedding into ERPX; generate original demo imagery and use ERPX-native layout/components.

This note is research and acceptance intent. Implementation, browser, integration and availability results must be recorded separately after verification. Prices, packages, country support and external documentation can change; recheck the selected provider at the actual contracting/integration phase.

## Official-source recheck — 2026-10-11 ICT

This recheck supplements the 2026-10-09 observations; it does not turn a public plan page into a binding quotation or a completed vendor demo.

| Source | Rechecked observation | Limit for selection/TCO |
| --- | --- | --- |
| [Cloudbeds language matrix](https://myfrontdesk.cloudbeds.com/hc/en-us/articles/1260804304509-Cloudbeds-PMS-supported-languages) | Thai is marked for PMS UI and help articles; the same row does not mark Activity Log, support/onboarding, training subtitles or Billing Portal. | Score Thai UI separately from Thai support service. |
| [Cloudbeds pricing](https://www.cloudbeds.com/pricing/) | Official indexed page still describes Flex, One, Experience and Enterprise with quote requests. Direct re-open timed out, so package details were rechecked through indexed official content. | Complete S1–S6 totals still require quotations. A separate vendor-hosted bot result contains conflicting package claims and is excluded from cost assumptions. |
| [Mews pricing](https://www.mews.com/en/pricing) | Core/Pro/Enterprise remain documented; room-based tailored quotes, separate processing fees and optional POS/bookable services are explicit. | Confirm Thailand, entitlements, minimums and provider fees in the quote. |
| [Mews language table](https://help.mews.com/s/article/language-support-in-mews?language=en_US) | Direct help rendering returned a CSS/loading error and another official link redirected repeatedly. Indexed official language-table content lists Unicode input but no Thai UI row. | This is limited indexed evidence, not a live Thai-language demo or proof that Thai input is impossible. |
| [Little Hotelier Thai pricing](https://www.littlehotelier.com/th/pricing/) | Flex/Pro, the displayed 1% booking-fee basis and 1–30-room positioning remain documented. Monthly amount placeholders are incomplete in the retrieved rendering. | Do not extrapolate the historic indicative homepage amount to 100/200-room hotels; request the applicable country/room-count quote. |
| [SiteMinder pricing](https://www.siteminder.com/pricing/) | Independent plans are room-count based; rendered amounts remain placeholders, while Groups & Chains directs to sales. | Price the underlying PMS and transactional/add-on costs separately. |
| [OPERA product](https://www.oracle.com/hospitality/hotel-property-management/hotel-pms-software/) | Current page documents property/chain data, company/agent profiles and rates, operations/POS integration and mobile guest workflows. | Product documentation does not establish a complete Thai tariff or purchased entitlements. |
| [Comanche Gogoji](https://www.gogoji.co.th/) | Retrieved current homepage describes hotel/group/residence/restaurant coverage, specialist setup/training and separate front-office/back-office support hours. | Detailed FOS/POS/accounting/inventory/API packages and the older precise room-range claim still need a vendor demo/written confirmation. |
| [SiteMinder integration requirements](https://developer.siteminder.com/pmsxchange-api/guides/integration-requirements) | XML flows require SOAP 1.1; selected REST operations are separately documented. Persistent retry queues, duplicate prevention and restart survival remain required. | Keep certification and partner activation after pilot; do not plan a REST-only OTA adapter. |

The six scenario totals and eight mandatory cost categories above remain **ต้องขอใบเสนอราคา / unknown** where the evidence cannot price a complete package. No vendor outreach, signup, credential request or commercial commitment was made.
