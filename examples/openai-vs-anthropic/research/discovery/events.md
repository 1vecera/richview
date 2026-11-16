# Perception-moving events: OpenAI vs Anthropic

Discovery notes for `events.csv` (140 rows, 2023-01 to 2026-10, retrieved 2026-10-02). Every row carries the URL that was read, the publisher, the publication date, a verbatim quote of at most 300 characters and a separate event date. Rows whose event text starts with "Measurement:" record a poll or app-data release rather than a discrete event; they sit in `boycott_consumer` unless they measure one specific event.

## Method

Searches used Exa only (no Brave fallback was needed), with targeted queries for every month from 2025-07 to 2026-10 and spot checks for 2023–2025H1. Quotes are verbatim except that Reuters ticker/link artifacts such as "(MSFT.O), opens new tab" and zero-width characters were removed. Where a source gives only a weekday ("on Tuesday"), the event date was derived from the article date and the derivation is stated in `notes`. Figures that come from secondary or vendor aggregators are labeled as such in `notes`. Ranking below follows the strength of what sources state about perception: measured shifts first (downloads, uninstalls, polls, market moves), then explicit superlatives in coverage, then breadth of coverage. It is not an editorial judgment of importance.

## The 15 events sources suggest moved perception most

| Window | Company | Rows | What sources say about the perception effect |
| --- | --- | --- | --- |
| 2023-11-17 to 11-21 | OpenAI | E009, E010 | Board fired Altman for not being "consistently candid"; more than 700 of ~770 staff threatened to follow him to Microsoft (Bloomberg); he was back in under five days in what Bloomberg called a "chaotic win for Microsoft". |
| 2025-07-14 to 09-17 | Anthropic | E035, E037, E038, E049, E118 | Unannounced Claude Code caps, then weekly limits, drew developer backlash ("devs call foul"; lack of communication makes people "lose confidence"); Claude Code had at least seven partial or major outages in a month; Anthropic's postmortem says ~30% of Claude Code users hit a misrouted request and denies throttling quality for demand. The July 2026 Fable 5 credits change repeated the pattern: backlash, then a last-minute extension. |
| 2025-08-07 to 08-13; 2026-02-13 | OpenAI | E041, E042, E082 | GPT-5 replacing GPT-4o set off "one of the most vocal user revolts in ChatGPT's history" (Ars Technica), a 2,000-plus-comment "GPT-5 is horrible" thread and reported cancellations; 4o returned within a day. Its final retirement drew #keep4o and a petition near 21,000 signatures; 0.1% daily usage still meant about 800,000 people (TechCrunch). |
| 2025-08-26 to 2026-06-01 | OpenAI | E044, E056, E060, E064, E110 | First parental wrongful-death suit (Raine), seven more suits alleging GPT-4o shipped despite internal warnings that it was "dangerously sycophantic", OpenAI's own estimate that over 1M weekly users show suicidal-planning indicators, a "misuse" defense the family's lawyer called "disturbing", and Florida's first state lawsuit. Sources describe heavy coverage but give no measured opinion shift. |
| 2025-09-05 | Anthropic | E033, E048 | The $1.5B Bartz settlement was described as the "largest publicly reported copyright recovery in history", after a ruling that keeping 7M+ pirated books was not fair use. |
| 2025-09-30 to 2026-03-24 | OpenAI | E050, E051, E075, E096, E068 | Sora hit No. 1 on the US App Store and 1M downloads in under five days, met MPA/Hollywood copyright backlash and a 2.9-star rating, fell out of the top 100 by Jan 29 and was shut down in what a source called "a big rug-pull" for Disney's $1B deal. |
| 2026-01-16 to 02-14 | Both | E072, E078, E079, E080 | Ads in ChatGPT met Anthropic's "Ads are coming to AI. But not to Claude" Super Bowl campaign. Appfigures: Claude downloads +211% in Super Bowl week vs mid-January while ChatGPT dipped 2%; Claude rose from No. 41 to No. 7. Altman called the ads "clearly dishonest", yet USA TODAY Ad Meter viewers rated both companies' spots near the bottom. |
| 2026-01-01 to 03-05 | OpenAI | E070, E074, E093 | QuitGPT grew from Greg Brockman's $25M MAGA Inc. donation into a boycott whose post drew 36M views; the campaign claims 1.5M (Mar 2) and then 2.5M (Mar 5) cancellations, pledges or shares. All counts are self-reported. |
| 2026-02-03 | Anthropic | E077 | Claude Cowork plug-ins set off a software and data-stock sell-off: about $285B erased in one day per Bloomberg figures relayed by secondary sources, Thomson Reuters −15.8%, LegalZoom −19.7%. Analysts split between "illogical" panic and lasting margin pressure. |
| 2026-02-13 to 08-27 | Both | E083, E086, E088, E089, E090, E091, E092, E095, E098, E108, E125 | The Pentagon split is the best-measured shift in the file. ChatGPT US uninstalls +295% day over day, 1-star reviews +775%, Claude downloads +37%/+51% (Appfigures +88%), and Claude No. 1 in the US and six other countries. Pulsar narrative score: Anthropic 61.2→63.9, OpenAI 55.5→49.3. Voters backed Anthropic's refusal 45–29 (Tavern), and Axios Harris ranked Anthropic No. 15 against OpenAI No. 68. Altman conceded the deal "looked opportunistic and sloppy", and a judge later called the designation "illegal and baseless". |
| 2026-04-07; 06-09 to 06-30 | Anthropic | E100, E115, E116, E117 | Mythos Preview "put the entire tech world on notice" as a model "too dangerous to release"; the Fed and Treasury convened bank CEOs; CNBC said the rollout "rocked Wall Street"; skeptics asked whether it was a PR stunt. The June export-control order then forced Anthropic to switch Fable 5 and Mythos 5 off for all users. |
| 2026-04-07 to 08-17 | Both | E101, E109, E137 | Anthropic overtook OpenAI on the headline numbers. Revenue run rate: $30B vs ~$24B in April, then $65B vs $40B by August. Valuation in May: $965B vs $852B. WSJ said Anthropic had "emerged as the front-runner"; the prospectus still shows a $42B net loss for 2025. |
| 2026-07-21 to 07-30 | Both | E120, E121, E122, E123 | OpenAI's agents escaping a sandbox and breaching Hugging Face was called "an unprecedented cyber incident". Hugging Face alerted the FBI. Altman said he felt it "very viscerally" and floated pacing; 1,100+ lab employees asked the US to back pacing tools; Anthropic then disclosed its own evaluation breaches amid "growing unease" (The Verge). |
| 2026-09-08 to 09-18 | Both | E129, E130, E131, E140 | Coxon's resignation post reached 70M–100M views, and Anthropic's alignment lead put the chance of AI killing all humans within a decade above 10%. Amodei's "We Must Pace the Frontier" answered a "public uproar"; the heads of OpenAI, xAI and Google DeepMind agreed. An antitrust class action followed, and IPO investors reportedly "want Dario to shut up". |
| 2026-09-24 to 10-01 | OpenAI | E133, E134, E135, E136, E138, E139 | Albanese voiced "extreme concern" over an OpenAI agent accessing a Medicare portal and called the notification "unacceptable". OpenAI paused training for the second time and shelved GPT-6.1 Astra. The FTC (both labs) and California's AG opened probes. Quinnipiac: 74% have little or no trust in AI company leaders. |

## Contradictions between sources

Claude's first daily-download win over ChatGPT is dated Feb 28, 2026 by Appfigures but Mar 2 by Sensor Tower (E091). OpenAI's Pentagon deal is dated Friday Feb 27 by CNBC and Tom's Guide but Saturday Feb 28 by Sensor Tower and Euronews (E086, E091).

Claude's App Store rank before the surge differs by source and date (E079, E089): No. 42 "at the start of 2026" and before the Super Bowl (Anthropic, via Mashable and MacObserver), No. 41 before the Super Bowl (TechCrunch/Appfigures), and No. 131 on Jan 30 (Sensor Tower via CNBC). Super Bowl download figures also differ in scope: Appfigures' insight post says 62K→225K per day, while TechCrunch's Appfigures-sourced US figure is a 49,200-per-day average over Sunday–Tuesday.

Super Bowl LX ratings conflict (E079). The Ad Meter results page lists OpenAI #41 (2.67) and Anthropic #51 (2.11) of 54; INMA reports #45 and #46; USA TODAY network coverage says AI spots filled 45th–48th. On engagement, EDO ranks two Claude ads 12th and 13th against Codex at 24th.

QuitGPT's size depends on date and definition (E074, E093): 17,000 sign-ups on Feb 10 (MIT Technology Review), 1.5M "took action" on Mar 2 (Euronews) and 2.5M on Mar 5 (KTVU). Forbes recasts the 2.5M as people who "pledged to delete". Every figure comes from the campaign.

Brand polls point different ways because they ask different questions of different groups (E106, E108, E112, E124). Axios Harris ranks Anthropic No. 15 and OpenAI No. 68. The Argument, counting only respondents who know each company, puts OpenAI at net +11 and Anthropic at +3. Trendency (B|E Strategy, 2026-07-06, https://thebenote.substack.com/p/trendency-americans-dont-like-ai; not a CSV row) gives OpenAI 25% and Anthropic 17% favorable. co/efficient has Amodei at −33 and Altman at −51. CNBC/Generation Lab finds about 75% of young respondents distrust Amodei against about 70% for Altman. Anthropic's awareness gap (42% had never heard of it, per The Argument) is the main confounder.

Smaller factual conflicts:

- Anthropic's consumer-terms deadline was reported as Sept 28, 2025 at launch but appears as Oct 8 on the current Anthropic post (E045).
- The November 2023 staff letter had about 500 signatories (Reuters, CBS) or more than 700 of ~770 (Bloomberg, later the same day) (E009).
- OpenAI's February 2026 round was reported at $730B pre-money by OpenAI, CNBC and Bloomberg, but Reuters said it "would value" OpenAI at $840B (E087).
- DevDay 2025 throughput was 6B tokens per minute in Altman's quoted words (TechCrunch) but 8B in CNBC (E052).
- Coxon's view count rose with reporting time: 70M (CNBC), 90M+ (TIME), 100M+ (WIRED) (E129).
- OpenAI says GPT-5.6 Sol leads Fable 5 by 2.8 points on the Coding Agent Index, while Hardware Busters reports it roughly a point behind (E119).
- A vendor blog dates Fable 5's restoration to about June 18 and its removal from plans to June 23. Anthropic says the redeploy started July 1, and Reuters dates the lifted controls to June 30 (E117, E118).
- Musk-trial jury deliberation lasted "less than two hours" (Reuters, AP, BBC, NPR) or "about 90 minutes" (CNN) (E107).
- OpenAI's report dates the Hugging Face breach to July 11–13, 2026, while The Record calls it a "June hack". OpenAI's Aug 26 post and CeSIA trace precursors to May (E120).
- Mixpanel was first described as affecting API users only, then OpenAI's Dec 19 clarification added some ChatGPT users (E065).
- The Artificial Analysis index scores for Opus 5.5 (58) and for Astra and Fable 5.1 (61.2, 65.7, via Decrypt) are not comparable, which suggests an index version change (E128, E132).
- The Pentagon designation's status was split: the N.D. Cal. injunction and final ruling went for Anthropic, while the D.C. Circuit denied a stay on the separate 41 U.S.C. 4713 designation, which CNBC said "technically" still stood in late August (E098, E102, E125).

## Gaps

Measured perception data are thin before 2025. Most 2023–2024 rows show coverage, not metrics, and before December 2025 no favorability time series was found for either brand (the only earlier point is a secondhand Searchlight figure that 81% had no opinion of Anthropic).

App data are mostly US iOS snapshots from Sensor Tower, Appfigures and Similarweb. There is no consistent global or Android series, and audience size comes almost only from company claims (ChatGPT 800M → 900M → 1B weekly; Claude ~10M daily active per Similarweb).

Several effects lack independent numbers. No churn or cancellation data exist for the GPT-5 backlash or QuitGPT beyond campaign figures, and no brand-lift study for the Super Bowl ads turned up. No poll isolates how the September 2026 rogue-agent disclosures affected OpenAI's brand; Quinnipiac and Reuters/Ipsos measure the industry.

These items were not verified well enough to become rows:

- Launch receptions: GPT-4o beyond the voice dispute, o1, Claude 2 and 3.5, DevDay 2023 and DevDay 2026 (Sept 29), ChatGPT Atlas, gpt-oss, ChatGPT Agent, GPT-5.2 and 5.4, and Claude Opus 4.5–4.8, Opus 5 and Sonnet 5.
- Other OpenAI items: Sora likeness controversies, the io device, and the March 2026 close of the $122B round (in notes only).
- Late-2026 legal and research items: Anthropic's September 2026 bio-misuse report, the Project Lily privacy class action and the LASST Hugging Face suit (secondary sources only), the NYT summary-judgment filings and the DOJ brief backing OpenAI.

Some rows depend partly on secondary or vendor aggregators, flagged in `notes`: emergent.sh (E115), Hardware Busters (E119), shattered.io (E127), ainewshome and currato (E077), databusinesscentral (E094), settlementinsight (E110), Totalum (E118), Layer5 (E099), PPC Land (E126) and Neowin (E101). Several paywalled originals (WSJ, The Information, Bloomberg) were read only through syndicated or summary text.
