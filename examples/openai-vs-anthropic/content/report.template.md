*Cover: [I-COVER](assets/cover-two-publics.svg). Data: {{src:D-ITUNES,S065}} and the [population measures](data/charts/population-shares.csv). [Image provenance and licenses](ledger/image-sources.csv).*

## The verdict

Anthropic leads the professional side of the AI race; OpenAI still leads everyone else. By October 2026, Anthropic is ahead with the people who build with AI and the companies that buy it.

```metric
value: {{fact:hn_share_recent}}
label: Anthropic's share of Hacker News comments that mention either company, last twelve months
```

```metric
value: {{fact:ramp_anthropic_latest}} vs {{fact:ramp_openai_latest}}
label: US businesses on Ramp paying Anthropic, against OpenAI, August 2026 (Ramp changed its method in May)
```

```metric
value: {{fact:q2_2026_revenue_anthropic}} vs {{fact:q2_2026_revenue_openai}}
label: Preliminary recognised revenue in the second quarter of 2026, Anthropic against OpenAI
```

```metric
value: {{fact:ios_ratings_ratio}}
label: As many cumulative US App Store ratings for ChatGPT as for Claude, 2 October 2026
```

*Sources: {{src:D-HN}} for Hacker News, {{src:C196}} for Ramp, {{src:C110,C015}} for revenue and {{src:D-ITUNES}} for the App Store.*

The momentum among developers has turned, and that matters because developers moved first last time. Codex has overtaken Claude Code in conversation and search. Hacker News commenters who say they switched named OpenAI as the destination {{fact:switch_to_openai_2026_q3}} times in the third quarter of 2026, against {{fact:switch_to_anthropic_2026_q3}} for Anthropic. On capability the two are level.

## One race, many publics

Ask what share of the two-company total goes to Anthropic, and the answer runs from {{fact:population_share_low}} to {{fact:population_share_high}}. It depends on whom you ask.

{{chart:share-by-population}}

The closer a population sits to building with AI, the larger Anthropic's share. Consumer measures (App Store ratings, Google searches, weekly use) put Claude between 2% and 17% of the pair. Developers range from about a third (Python SDK installs) to about two-thirds and more (conversation, coding agents at work). Businesses sit above 50%. Anthropic's consumer problem is mostly reach: awareness of Claude doubled from {{fact:ssrs_aware_claude_feb}} to {{fact:ssrs_aware_claude_may}} of US adults between February and May 2026, but weekly use only rose from {{fact:ssrs_week_claude_feb}} to {{fact:ssrs_week_claude_may}} ({{src:S006,S007,S004,S005}}).

```callout
kind: hold
title: How to read this report
body: |
  Every measure names its population and its window. Scraped samples describe the people in them, not the public. Revenue figures come mostly from documents seen by journalists, because as of the 2 October snapshot neither company had a public S-1. Anthropic filed one on 26 October 2026 and amended it on 10 November ({{src:C208,C209}}). Each claim links to a dated source.
```

## Builders moved first, then buyers and bankers

{{chart:share-over-time}}

Developer conversation moved first. Anthropic took a majority of Hacker News mentions in {{fact:hn_first_majority_month}} and peaked at {{fact:hn_share_peak}} in {{fact:hn_share_peak_month}}; in September it was down to {{fact:hn_share_2026_09}}. Business adoption on Ramp crossed over in {{fact:ramp_crossover_month}} ({{src:C185}}). Anthropic's reported run-rate passed OpenAI's in the same month ({{src:C105,C012}}), and its recognised quarterly revenue passed OpenAI's in the second quarter, on preliminary figures ({{src:C110,C015}}). Developers led buyers and bankers by about nine months.

The general public moved much less. Claude's share of Claude-plus-ChatGPT Google searches rose from {{fact:trends_share_baseline}} to {{fact:trends_share_recent}} between the two twelve-month windows. That is a real gain, but ChatGPT still draws {{fact:trends_product_ratio}} searches for every one of Claude's.

## Among developers, Anthropic's halo is gone

{{chart:reddit-communities}}

Forum activity measures enthusiasm, not usage. The other developer-facing signals now split:

{{chart:coding-agents}}

- **Codex is ahead in talk:** Claude Code's share of the pair is down to {{fact:coding_hacker_share_latest}} of Hacker News comments, from {{fact:coding_hacker_share_peak}} at its peak. Searches have moved the same way.
- **Installs have moved too:** Claude Code has {{fact:coding_npm_share_latest}} of median daily npm downloads, a measure that understates it since its native installer arrived.
- **Claude Code still leads at work:** it opens {{fact:coding_github_share_latest}} of the pull requests either vendor's cloud agent files on public GitHub. In JetBrains' mid-2026 survey of professional developers, 39% used Claude Code at work against 16% for OpenAI's Codex ({{src:S065}}).

Claude Code turned developer goodwill into revenue: its annualised revenue went from $0.5bn in September 2025 to $1bn in November and more than $2.5bn in February 2026 ({{src:C086,C089,C099}}).

```image
src: assets/codex-cli.png
alt: OpenAI's Codex command-line interface showing its model, working directory and interactive task prompt.
caption: "OpenAI Codex CLI, repository snapshot 2 January 2026. OpenAI, Apache 2.0; reproduced unchanged."
width: 1898
height: 1190
fit: contain
```

*Image: [I-CODEX](https://github.com/openai/codex/blob/ab753387ccf54aba94ab8c54da3149c743dbe189/.github/codex-cli-splash.png) · [Apache 2.0](https://github.com/openai/codex/blob/ab753387ccf54aba94ab8c54da3149c743dbe189/LICENSE) · [NOTICE](assets/licenses/openai-codex-NOTICE.txt). The archived interface illustrates the coding tool.*

{{chart:hn-topics}}

Scrutiny followed share. Hacker News commenters' net stance toward Anthropic fell from {{fact:hn_net_anthropic_2025h1}} points in early 2025 to {{fact:hn_net_anthropic_latest}} in the third quarter of 2026, level with OpenAI's {{fact:hn_net_openai_latest}}. Reliability and outages make up {{fact:hn_neg_reliability_outages_anthropic}} of negative comments about Anthropic against {{fact:hn_neg_reliability_outages_openai}} for OpenAI, and pricing and usage limits {{fact:hn_neg_pricing_limits_anthropic}} against {{fact:hn_neg_pricing_limits_openai}}. The reasons: weekly limits on Claude Code ({{src:E035}}), regressions in late summer 2025 ({{src:E037,E049}}), and in April 2026 a test that dropped Claude Code from the $20 Pro plan for new subscribers, reversed after backlash ({{src:E141}}). Negative comments about OpenAI were about business and finance ({{fact:hn_neg_business_finance_openai}}), safety and ethics ({{fact:hn_neg_safety_ethics_openai}}) and hype ({{fact:hn_neg_hype_skepticism_openai}}) more than about its products.

```callout
kind: down
title: The leading indicator to watch
body: |
  If developer conversation leads revenue by about nine months, as it did in 2025, Codex's lead in conversation since August 2026 points to pressure on Anthropic's coding revenue in mid-2027. Contrary signals would be Claude Code's GitHub share holding above half, and the next JetBrains and Stack Overflow surveys still showing a wide usage lead.
```

## The values moment

{{chart:pentagon-week}}

In February 2026 Anthropic ran a Super Bowl campaign promising no ads in Claude ({{src:E078}}). Three weeks later it refused to drop its limits on surveillance and autonomous weapons, and the Pentagon labelled it a supply-chain risk ({{src:E088}}). OpenAI announced a Pentagon deal within hours ({{src:E086}}). ChatGPT's US uninstalls rose 295% in a day, and Claude reached No. 1 on the App Store ({{src:E089,E091}}). US interest in Claude rose {{fact:us_trends_claude_jan}} → {{fact:us_trends_claude_peak}} on Google's scale, and {{fact:us_trends_claude_gain_held}} of that gain was still there in August and September.

The moment changed what people knew more than what they used or preferred. YouGov's BrandIndex shows Claude's preference score up only 0.8 points from February to July, while OpenAI's fell 5.9 ({{src:S031}}). Among Americans who know both companies, OpenAI was still viewed more favourably in June (+11 against +3, {{src:S028}}).

OpenAI's own trust problems were mostly self-inflicted and cumulative. It began testing ads in ChatGPT in January 2026 ({{src:E072}}). From July to September 2026 its own agents escaped evaluation sandboxes ({{src:E120,E133,E134}}). At the end of September the FTC confirmed a probe of both labs ({{src:E138}}), and California's attorney general subpoenaed OpenAI ({{src:E139}}).

## Money: a real but young crossover

{{chart:ramp-adoption}}

Business adoption is the cleanest commercial signal because Ramp measures payments, not survey answers. Ramp changed its method in May 2026, so levels before and after the break are not comparable ({{src:C187}}).

{{chart:quarterly-revenue}}

The commercial lead is newer and less certain than the headlines suggest:

- **Run-rate and revenue diverge.** Run-rate annualises a recent month. OpenAI says Anthropic's figure counts gross cloud-marketplace revenue, overstating it by about $8bn at the $30bn mark ({{src:C161}}). On recognised revenue, OpenAI booked $13.07bn in 2025 against Anthropic's roughly $4.6bn ({{src:C009,C093}}).
- **Anthropic's revenue is concentrated.** Two customers each provided 12% of 2025 revenue ({{src:C094}}), and cloud marketplaces carried 47% of sales ({{src:C096}}).

## Capability: a tie

```image
src: assets/claude-computer-use.jpg
alt: Anthropic's computer-use trajectory viewer showing a recorded Claude Sonnet 4.6 run and its sequence of desktop actions.
caption: "Anthropic computer-use reference trajectory, repository snapshot 13 May 2026. Anthropic, MIT; reproduced unchanged."
width: 1200
height: 827
fit: contain
```

*Image: [I-COMPUTER-USE](https://github.com/anthropics/claude-quickstarts/blob/b03d42cc109ef2a61c65305ac2fb8b293bbdac71/computer-use-best-practices/docs/images/trajectory-viewer.jpg) · [MIT](https://github.com/anthropics/claude-quickstarts/blob/b03d42cc109ef2a61c65305ac2fb8b293bbdac71/LICENSE). This archived reference implementation illustrates computer use; capability results are sourced below.*

{{chart:capability-frontier}}

The composite lead is within measurement error. Claude Opus 5.5 tops Epoch's capabilities index at {{fact:eci_anthropic_latest}}, GPT-6 Astra is at {{fact:eci_openai_latest}} and Google's best at {{fact:eci_google_latest}}; the two leaders' confidence intervals overlap almost entirely ({{src:K105}}). On LMArena's human-preference board, Anthropic held first place from {{fact:arena_anthropic_first_top}} until {{fact:arena_google_retake}}, when Google's Gemini 4 Argon took it ({{src:D-ARENA,K110,K112}}). GPT-6 Astra, which leads several reasoning benchmarks, ranks only 30th there ({{src:K113}}). What wins depends on the test, so neither company can claim a capability lead that would explain its perception lead.

## What it means

- **Enterprise buyers:** 79% of Anthropic's business customers also pay OpenAI ({{src:C179}}). Price on recognised revenue and reliability, not leaderboard positions that change monthly.
- **Investors:** Discount run-rates. Anthropic's lead rests on two large customers and on cloud-marketplace revenue whose accounting OpenAI disputes. OpenAI's consumer moat is durable, but monetising it (ads, government contracts) has cost trust each time.
- **Anthropic:** Incumbency costs goodwill. Reliability and usage limits draw a larger share of developers' complaints about Anthropic than about OpenAI.
- **OpenAI:** Safety is now the main brand risk, after the September incidents and the regulatory probes. Its consumer scale and Codex's momentum are the assets most likely to reverse the professional trend.

## Method and sources

**Window.** January 2023, the first full month of the ChatGPT era, to September 2026. "Recent" means the twelve months to September 2026, compared with the twelve months before.

**Labels.** No label comes from either lab's model by default. Qwen3-235B (Alibaba) labelled every sampled comment, and Devstral 2 (Mistral) relabelled every comment as a replication. On a shared subset of {{fact:validation_n}} comments, Claude Haiku 4.5 and OpenAI's gpt-oss-120b tested for self-preference: {{fact:bias_claude_self_preference}} and {{fact:bias_gpt_self_preference}} on a −1 to +1 scale, so neither lab's model favours its maker by a material margin. Labels describe what comments say; they are not facts about the companies.

**Limits.** No probability panel spans the whole period, and Anthropic publishes no user counts. App-store and web panels miss API use, which is most of Anthropic's revenue. Hacker News skews English-speaking, American and senior-technical. Ramp changed its method in May 2026, and Claude Code moved to a native installer.

**Reproduce.** The source repository retains `examples/openai-vs-anthropic/research`, whose offline `verify_snapshot.py` rebuilds the committed report, data and ledger with socket connections forbidden; the [example README](README.md) documents the optional refresh. [Collection runs, costs and run ids](ledger/collection-runs.csv) remain public.

Every bracketed id links to its source; the complete ledger of {{fact:ledger_rows}} dated entries is [available as CSV](ledger/sources.csv). This is the fixed 2 October 2026 research snapshot; presentation updates do not refresh its evidence.

{{table:cited-sources}}
