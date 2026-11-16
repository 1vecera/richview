# OpenAI/ChatGPT vs Anthropic/Claude: survey and poll evidence

Discovery notes for `surveys.csv` (94 rows, S001–S094), compiled 2026-10-02. Every figure comes from a source retrieved in this session. Ratios marked "computed" are simple divisions of ledger values, not published figures. Row ids refer to `surveys.csv`.

## Bottom line

In probability samples of U.S. adults, ChatGPT reaches 5–7 times as many people as Claude in 2026. In developer samples the gap shrinks to under 2:1, and for coding agents it reverses in Claude's favour. Business-adoption evidence is split, and the answer depends on who is measuring. Claude's awareness roughly doubled in 2026, but measures of preference and favourability barely moved.

- **Public (probability samples):** ever use 44% vs 6% (Pew, Feb 2026, S002; 7.3:1, computed). Past-week use 43% vs 9% (SSRS, May 2026, S005; 4.8:1). Past-week use about 30% vs 4.3% (Epoch AI/Ipsos KnowledgePanel, Apr 2026, S013). In mid-2025 the six-country average from the Reuters Institute/YouGov survey was 22% vs 1% weekly use (S015).
- **Developers (opt-in):** Stack Overflow 2025 model use was OpenAI GPT 81.4% vs Claude Sonnet 42.8% (S054, 1.9:1). Claude Sonnet was more admired (67.5% vs 61.2%, S056). JetBrains' 2026 survey has Claude Code used at work by 39% vs 16% for OpenAI Codex (S065). ChatGPT still wins as a chat interface for coding, 28% vs 7% (JetBrains, Jan 2026, S063).
- **Perception:** Claude awareness went from 21% to 42% of U.S. adults between February and May 2026 (S006, S007). Its preference share among AI users rose only 0.8 points from February to July (S031). Among people aware of each company, net favourability was OpenAI +11 vs Anthropic +3 (S028). Claude leads satisfaction among its own users (59.3 vs 53.5, S035), which reflects who its users are, not reach.
- **Pentagon dispute:** several mostly non-probability polls found pluralities backing Anthropic's position over the Pentagon's, by 44–50% vs 25–35% (S019–S022). A context-framed opt-in item reached 85% (S023). The available brand metrics show no matching gain in Claude preference or favourability (S028, S031).

## Method categories used in the ledger

`method` records what the source itself claims; ambiguity is spelled out in `notes`.

- **`probability`:** the source states probability-based recruitment. Examples are the Pew ATP, SSRS Opinion Panel, Ipsos KnowledgePanel, and Elon/SSRS.
- **`nationally_representative_panel`:** the sample is claimed to be nationally representative through quotas or weights, with no probability claim. Examples are Reuters/YouGov, Economist/YouGov, Morning Consult/Menlo and ITIF, EmpathixAI and Second Raven. These range from strong (Reuters, Economist/YouGov) to doubtful (Second Raven, EmpathixAI).
- **`opt_in_panel`:** an online non-probability sample with no representativeness claim, or one the source explicitly calls non-probability. Examples are Echelon, Searchlight/Tavern, Change Research and Tavern.
- **`professional_opt_in`:** self-selected developer, professional or buyer samples, even when weighted. Examples are Stack Overflow, JetBrains, Pragmatic Engineer, Devographics, Retool, Menlo, a16z and Morning Consult B2B.
- **`telemetry_estimate`:** passive or modelled measurement such as Similarweb, Sensor Tower, Appfigures, Ramp card spend, Yipit and UKOM/Ipsos iris.
- **`other`:** the method is not disclosed in the retrieved text. Examples are YouGov BrandIndex, Ratings and AI Index; The Argument; Trendency; the Morning Consult consumer studies; and CNBC/Generation Lab.

For public-facing headline claims, use only `probability` rows: S002, S005, S007 and S013 as the core, with S009, S012 and S014 as supporting rows. Treat all other public rows as directional.

## Representative samples: the public picture

| Measure (row) | Fieldwork | ChatGPT | Claude | Ratio (computed) |
|---|---|---|---|---|
| Pew, ever use, U.S. adults (S002) | Feb 17–23, 2026 | 44 | 6 | 7.3:1 |
| Pew, ever use, ages 18–29 (S003) | Feb 2026 | 61 | 10 | 6.1:1 |
| SSRS, past week, U.S. adults (S004) | Feb 5–9, 2026 | 36 | 4 | 9:1 |
| SSRS, past week, U.S. adults (S005) | May 14–18, 2026 | 43 | 9 | 4.8:1 |
| SSRS, past week, business use (S008) | May 2026 | 19 | 5 | 3.8:1 |
| Epoch/Ipsos, past week (S012) | Mar 3–6, 2026 | 31 | 3 | ~10:1 |
| Epoch/Ipsos, past week (S013) | Apr 3–5, 2026 | ~30 | 4.3 | ~7:1 |
| Epoch/Ipsos, self-paid subscription (S014) | Mar 2026 | 4 | 1 | 4:1 |
| Elon/SSRS, LLM users ever used (S009) | Jan 2025 | 72 | 9 | 8:1 |
| Elon/SSRS, LLM used most (S010) | Jan 2025 | 52 | <0.5 | — |
| SSRS, heard of (S007) | May 2026 | 90 | 42 | gap 48 pts |
| Reuters/YouGov, weekly, six-country average (S015) | Jun–Jul 2025 | 22 | 1 | 22:1 |
| Reuters/YouGov, heard of, USA (S016) | Jun–Jul 2025 | 73 | 13 | — |

ChatGPT's lead is largest in reach and narrows on intensity of use. On work use (S008) the ratio is about 4:1, and on self-paid subscriptions (S014) also about 4:1 from very small bases. Claude's weekly users are more work-oriented than ChatGPT's: about 5 of 9 cite business use, against 19 of 43 for ChatGPT (computed). Copilot is even more work-oriented, at 11 of 17. This fits YouGov's knowledge-worker tilt for Claude (S037) and Morning Consult's report that Claude's active use among $100K+ earners nearly doubled to 17.1% (source of S043).

Company-level awareness and favourability come only from non-probability or undisclosed-method polls. In August 2025, 81% had not heard enough about Anthropic to form a view, vs 42% for OpenAI (Searchlight, S024). That 81% combines 55% who had never heard of Anthropic with 26% who had heard of it but had no opinion. Net favourability was then +9 for OpenAI and −1 for Anthropic (S025). By late May to early June 2026, 42% had not heard of Anthropic vs 13% for OpenAI (The Argument, S027), with net favourability among the aware of +11 vs +3 (S028). Trendency puts both labs underwater, at 25% favourable for OpenAI vs 17% for Anthropic (S029). There is no probability-based company-favourability measure for either lab.

## Developer and professional samples

| Measure (row) | Fieldwork | OpenAI | Anthropic | Note |
|---|---|---|---|---|
| SO 2024, AI tools used: ChatGPT vs Claude (S052) | 2024 | 82.1 | 8.1 | brand-level item |
| SO 2025, LLMs used, all (S054) | May–Jun 2025 | 81.4 | 42.8 | model families; n=16,474 |
| SO 2025, LLMs used, professionals (S055) | May–Jun 2025 | 81.9 | 44.9 | learners 81.8 vs 29.8 |
| SO 2025, admired (S056) | May–Jun 2025 | 61.2 | 67.5 | Claude ahead |
| SO 2025, agent users: ChatGPT vs Claude Code (S057) | May–Jun 2025 | 81.7 | 40.8 | product mismatch |
| SO pulse, code assistants: Codex vs Claude Code (S059) | Apr 2026 | 20 | 51 | n=1,100 |
| JetBrains 2024, regular use (S061) | 2024 | 49 | 2.7 | Claude figure secondary |
| JetBrains 2025, regular use (S062) | Apr–Jun 2025 | 41 | 8 | secondary |
| JetBrains AI Pulse, chatbot for coding at work (S063) | Jan 2026 | 28 | 7 | weighted, >10k |
| JetBrains AI Pulse, coding agent at work (S064) | Jan 2026 | 3 | 18 | Codex vs Claude Code |
| JetBrains 2026, coding tool at work (S065) | May–Jul 2026 | 16 | 39 | Claude Code in the US: 47 |
| Devographics State of AI, ever used (S067) | 2025 | 91.2 | 57 | web-developer opt-in |
| Pragmatic Engineer, most loved (S066) | Jan–Feb 2026 | — | 46 | Cursor 19, Copilot 9 |

How big the developer gap looks depends on the sample. In enthusiast-heavy opt-in surveys it is 1.6–2:1 for general model use (S054, S067). In JetBrains' weighted samples the chatbot gap is 4–5:1 (S062, S063). Both are well below the 5–7:1 seen among U.S. adults in 2026, and the 22:1 seen in the six-country panel in 2025. For agentic coding, Claude leads 2.4–2.6:1 (S059, S065). Developer samples should not stand in for the public; they over-represent heavy and early adopters. The Pragmatic Engineer sample is a newsletter audience, Devographics is the JavaScript community, and Stack Overflow is recruited through its own site. JetBrains is the most defensible developer source: it uses regional quotas, weighting, and debranded recruitment for its AI Pulse waves (S063–S065).

## Enterprise buyers and business adoption

The evidence conflicts and is shaped by investors' positions; do not cite a single "enterprise share" without its definition.

- **Menlo Ventures (investor in Anthropic):** Anthropic 32% vs OpenAI 25% of enterprise production LLM usage in mid-2025 (n=150, S069). Anthropic 40% vs OpenAI 27% of estimated enterprise LLM API spend in November 2025 (n=495, S071). Coding share 54% vs 21% (S072). Menlo's report flags Anthropic as a "Menlo Ventures investment".
- **a16z (investor in OpenAI, per The Decoder):** 78% of 100 Global 2000 CIOs run OpenAI in production vs 44% for Anthropic (S073). OpenAI holds about 56% of model wallet share (S074). Yipit panel data cited by a16z put adoption at OpenAI ~85% vs Anthropic ~55% (S075).
- **Ramp card and bill-pay data (70,000+ Ramp customers):** Anthropic 43.8% vs OpenAI 39.8% of businesses paying for each lab's subscriptions or tokens in August 2026 (S077). Ramp's customers skew tech-forward, and its data miss OpenAI usage bundled through Microsoft and free tiers.
- **Morning Consult (mindshare among SMB decision-makers):** OpenAI 12.6% vs Anthropic 2.2% of mental market share (S076). This measures which brand comes to mind, not usage.

The pattern is consistent: Anthropic is strongest in API and coding spend and among tech-forward buyers. OpenAI is strongest in breadth of deployment, total wallet including chat seats, and mindshare.

## Consumer reach: web and app telemetry

- **Web traffic (Similarweb, worldwide, web only):** chatgpt.com averaged 5.7B monthly visits vs 331M for claude.ai over June 2025–May 2026, about 17:1 (S079). In traffic share, Claude rose from 1.9% to 9.3% between August 2025 and August 2026, while ChatGPT fell from 73.3% to 55.5% (S081). Most of ChatGPT's lost share went to Gemini (S080).
- **Retention and overlap (Similarweb):** six-month retention is 59% for ChatGPT vs 45% for Claude (S083). 61% of Claude's users also use ChatGPT in the same month, and 14% of ChatGPT's users also use Claude (S084). Claude is mostly an addition to ChatGPT, not a substitute.
- **Rankings (a16z, data from January 2026):** ChatGPT is about 30x Claude on the web (S085) and has about 8x as many U.S. paid subscribers (S086).
- **Apps (Appfigures):** in April 2026, Claude's peak download month, worldwide downloads were 53M for ChatGPT vs 20M for Claude (S087). Mobile revenue in August 2026 was $230M vs $82M net (S090), a 2.8:1 gap, far smaller than the reach gap. Claude earns more per download ($5.92 vs $4.29).
- **The Pentagon spike:** Claude overtook ChatGPT in daily U.S. downloads at the end of February or early March 2026 (S091). ChatGPT was back ahead later in March (S092).

## Trend over time

- **ChatGPT reach in the U.S.:** ever use rose from 18% (July 2023) to 23% (February 2024), 34% (2025) and 44% (2026, after a wording change) (S001, S002). Weekly use was 36% in February 2026 and 43% in May (S004, S005).
- **Claude reach among the public:** about 1% weekly use in both 2024 and 2025 across six countries (S015). Among LLM users, 9% had ever used it in January 2025 (S009), and 4% of AI users had in August 2025 (S026). Ever use was 6% of U.S. adults in February 2026 (S002). Weekly use rose from 3.0% to 4.4% to 4.3% across the March–April 2026 Epoch/Ipsos waves (S013), and from 4% to 9% between SSRS's February and May 2026 waves (S004, S005). Among AI-using likely voters (Echelon, opt-in), Claude went from 8% in February to 11% in August and 12% in September 2026 (S047–S049). Different questions are not like-for-like; the SSRS waves are the cleanest within-series trend.
- **Claude awareness:** 13% of U.S. respondents in mid-2025 (S016), then 21% in February 2026 and 42% in May 2026 (S006, S007).
- **Anthropic company awareness:** the share who had never heard of Anthropic went from 55% (August 2025, Searchlight crosstab, S024) to 42% (May–June 2026, The Argument, S027). That is about 13 points over roughly 10 months, from two different pollsters. The Argument's headline gain of 39–53 points over six months compares Searchlight's 81% "no opinion" (never heard plus heard but no opinion) with "not heard of", and misdates Searchlight's fieldwork.
- **Developers:** Claude went from a niche tool (8.1% in Stack Overflow 2024, S052; 2.7% in JetBrains 2024, S061) to a mainstream model (42.8% in Stack Overflow 2025, S054). Claude Code went from about 3% (April–June 2025) to 18% (January 2026) to 39% (May–July 2026) (S064, S065). OpenAI's Codex responded, rising from 3% to 16% over the same 2026 window (S065).
- **Enterprise, by Menlo's series:** Anthropic's share went from 12% (2023) to 24% (2024), 32% (mid-2025, usage share) and 40% (late 2025, estimated spend share) (S069, S071). The metric changes partway through the series. Ramp has Anthropic overtaking OpenAI among its customers during 2026 (S077, S078).

## Contradictions and measurement traps

- **Claude awareness varies with the sampling method.** The probability estimate is 42% (SSRS, May 2026, S007). Other sources at similar dates put it at 32% (Morning Consult, n=302, S043), 59.3% (YouGov BrandIndex, ambiguous base, S032), 63% (YouGov Ratings "fame", S039) and 80.6% (EmpathixAI quota sample, S045). Most opt-in sources run well above the probability figure, which fits online panellists being more tech-engaged. The one lower value comes from a small study with an unclear source.
- **Gemini outranks ChatGPT in Echelon's opt-in omnibus** (57–59% vs 50–53% among AI users, S047–S049), unlike every probability source (S002, S005, S012). The likely cause is item wording ("Gemini from Google" picks up AI Overviews) or the sample.
- **Menlo and a16z give opposite enterprise answers** (S071 vs S074), and the two investors back opposite labs. Their scopes also differ: API spend vs total model wallet, n=495 decision-makers vs 100 CIOs.
- **Stack Overflow is internally inconsistent.** Its 2025 text says Claude Sonnet is the most admired model "behind Gemini Reasoning", but its own chart shows 67.5 vs 65.2 (S056). The same April 2026 pulse is reported as Claude Code at 51% (S059) and at 55% (S060).
- **Similarweb's own posts disagree.** August 2026 shares are 55.5%/9.3% (S081) vs 57%/9.6% (S082), and the year-ago ChatGPT share is 73.3% vs 78%. ChatGPT's absolute visits are described as "+53%" (S079), "roughly flat" (S080) and "declined year over year" (S082), probably reflecting different windows and competitor sets.
- **Two vendors date the first day Claude beat ChatGPT in U.S. downloads differently:** Appfigures says February 28 and Sensor Tower says March 2, 2026 (S091).
- **The Searchlight poll is misdated and mismatched in secondary coverage.** It was fielded August 1–6, 2025 but is cited as a "December 2025" poll. It is also compared on "no opinion" (81%) rather than "never heard of" (55%), which inflates the claimed gain in Anthropic awareness from about 13 points to 39–53 (S024, S027).
- **Ramp's releases do not reconcile.** Its June and July 2026 releases both say OpenAI fell 0.1 points to 39.5%, after a methodology change (S078). Its summer report says Anthropic passed OpenAI "for the first time", while the June release says Anthropic was "remaining the leader".
- **Morning Consult's text and chart diverge.** A memo says "only Gemini gained" net trust, but the chart shows Anthropic +1.4 and Apple Intelligence +1.2 (S042).
- **Epoch AI/Ipsos posted a topline with the wrong fieldwork dates (April vs March 2026),** later corrected (S012).
- **Dispute polls depend heavily on wording.** Support for the abstract principle is 44% vs 25% (S019). Naming both sides gives 47% vs 31% (S020). Supplying context gives 85% saying Anthropic's restrictions are "reasonable" (S023, a low-quality sample).
- **High satisfaction does not mean high preference.** Claude tops net satisfaction among its users (S035) but has only a 6.1% preference share (S030), because satisfaction is measured on self-selected user bases.
- **Some articles labelled "Stack Overflow 2026" reuse 2025 data** (for example byteiota and Cadence). As of 2026-09-30, Stack Overflow said its 2026 results were still forthcoming (S060).

## Gaps

- **No probability-based company favourability** for OpenAI vs Anthropic exists before or after the Pentagon dispute; Pew, Gallup and AP-NORC did not ask. The only post-dispute favourability figures come from sources with undisclosed methods: The Argument (S027, S028) and Trendency (S029).
- **No 2026 cross-national brand-level public data.** The Reuters "Generative AI and News" 2026 edition was not found, and the Digital News Report 2026 gives no per-brand split. In the 2025 Reuters report, country-level weekly use per tool appears only as an unlabelled chart (S015). UK passive measurement does not publish Claude audience levels (S093, S094).
- **Pew 2026 gives ever-use only,** not use frequency by brand.
- **No current probability frequency data after May 2026:** SSRS's twice-monthly AI User Metrics waves are subscription-only after May 2026.
- **Stack Overflow 2026 main survey not yet published.** JetBrains 2026 chatbot shares (ChatGPT vs Claude) were not in the posts retrieved.
- **Enterprise evidence comes from small, investor-run surveys** (n=100–495) with conflicts of interest, plus Ramp's skewed customer base. No public Gartner, KPMG or Edelman brand-level data was found. KPMG's AI Pulse and the Edelman 2026 Trust Barometer report no lab-level splits.
- **Telemetry misses most of Claude's usage.** Similarweb excludes apps and API, and Sensor Tower publishes only day-over-day changes, not levels. Anthropic's revenue is mostly API (per Similarweb's IPO post), so consumer telemetry understates its business usage.
- **Morning Consult's absolute net-trust levels for both labs are not public** (S042).

## Down-weighted or excluded sources

- **Excluded:** the SmarterX "AI Pulse" listener poll (n=91, informal), articles mislabelled as Stack Overflow 2026, and secondary blogs citing model names or figures that could not be traced to a primary source (for example thrumos.com and ai-jarvis.eu beyond their Similarweb numbers).
- **Not in the ledger because no OpenAI/Anthropic split was retrieved:** AP-NORC (Jul 10–14, 2025; Jul 29–Aug 10, 2026), the Gallup workforce panel (Feb 4–19, 2026, n=23,717), Washington AI Network × Morning Consult (May 27–30, 2026, n=1,501), the Ipsos AI Monitor 2026, Edelman 2026 and KPMG AI Pulse Q3 2026. YouGov's "Anthropic Public Record" (n=51,993, Nov 1–Dec 11, 2025) is commissioned by Anthropic and has no retrieved brand comparison.
- **In the ledger but low weight:** Trendency (S029), EmpathixAI (S045, S046), Second Raven (S023), CNBC/Generation Lab (S051, secondary) and the Morning Consult n=302 study (S043, S044).
