---
title: Word cloud
description: Word cloud for the words or topics of a text sized by how often they occur.
styles: [scientific]
---

Use a word cloud for a first look at which words matter most in a text or a list of topics. Reach for a bar chart when readers need to compare the values.

```chart
type: wordcloud
title: Dashboards and forecasts lead what readers ask for
subtitle: Requests per topic over one quarter, illustrative counts
data:
  inline:
    - {topic: dashboards, requests: 48}
    - {topic: forecast, requests: 41}
    - {topic: churn, requests: 36}
    - {topic: revenue, requests: 33}
    - {topic: cohort, requests: 28}
    - {topic: funnel, requests: 25}
    - {topic: margin, requests: 22}
    - {topic: retention, requests: 20}
    - {topic: pricing, requests: 18}
    - {topic: segment, requests: 16}
    - {topic: anomaly, requests: 14}
    - {topic: budget, requests: 12}
    - {topic: trend, requests: 11}
    - {topic: pipeline, requests: 10}
    - {topic: ratio, requests: 9}
    - {topic: outlier, requests: 8}
    - {topic: survey, requests: 7}
    - {topic: lift, requests: 6}
    - {topic: mix, requests: 5}
    - {topic: drift, requests: 4}
encoding: {label: topic, y: requests}
axes:
  y: {label: Requests}
```

## Source

````rmd
```chart
type: wordcloud
title: Dashboards and forecasts lead what readers ask for
subtitle: Requests per topic over one quarter, illustrative counts
data:
  inline:
    - {topic: dashboards, requests: 48}
    - {topic: forecast, requests: 41}
    - {topic: churn, requests: 36}
    - {topic: revenue, requests: 33}
    - {topic: cohort, requests: 28}
    - {topic: funnel, requests: 25}
    - {topic: margin, requests: 22}
    - {topic: retention, requests: 20}
    - {topic: pricing, requests: 18}
    - {topic: segment, requests: 16}
    - {topic: anomaly, requests: 14}
    - {topic: budget, requests: 12}
    - {topic: trend, requests: 11}
    - {topic: pipeline, requests: 10}
    - {topic: ratio, requests: 9}
    - {topic: outlier, requests: 8}
    - {topic: survey, requests: 7}
    - {topic: lift, requests: 6}
    - {topic: mix, requests: 5}
    - {topic: drift, requests: 4}
encoding: {label: topic, y: requests}
axes:
  y: {label: Requests}
```
````

## Fields that matter

- `type: wordcloud` with `encoding.label` for the word and `encoding.y` for its value, such as a count
- A word's size follows the square root of its value between the smallest size the chart text allows and a largest that fits the plot, on one scale in every panel; words are set horizontally and coloured along the style's ramp by the same value
- Words go in from the largest to the smallest at the first free place out of the centre, so the same data always gives the same picture
- A word that finds no room is not drawn and is reported, and stays in the readouts and the table; a panel takes at most 150 words, and above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- A row without a word or a value above zero is left out and reported
- The share in a readout is computed by RichView and marked as derived
- `encoding.facet` makes one cloud per value; `axes.y.label` names the value

## Interactions

- Hover or focus a word to read its value and its share.
- Open the Table view, Download PNG or CSV, or Expand the chart.
