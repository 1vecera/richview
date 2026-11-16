---
title: Alluvial diagram
description: Alluvial diagram for how rows flow through several categorical fields, with ribbons between neighbouring categories.
styles: [scientific]
---

Use an alluvial diagram to show how the same items are spread across several categorical fields, such as the plan, channel and outcome of each user.

```chart
type: alluvial
title: Referred users stay more often than users who came from ads
subtitle: Users by plan, acquisition channel and outcome, synthetic
data:
  inline:
    - {plan: Free, channel: Search, outcome: Stayed, users: 120}
    - {plan: Free, channel: Search, outcome: Left, users: 180}
    - {plan: Free, channel: Ads, outcome: Stayed, users: 60}
    - {plan: Free, channel: Ads, outcome: Left, users: 140}
    - {plan: Free, channel: Referral, outcome: Stayed, users: 70}
    - {plan: Free, channel: Referral, outcome: Left, users: 30}
    - {plan: Basic, channel: Search, outcome: Stayed, users: 90}
    - {plan: Basic, channel: Search, outcome: Left, users: 40}
    - {plan: Basic, channel: Ads, outcome: Stayed, users: 50}
    - {plan: Basic, channel: Ads, outcome: Left, users: 50}
    - {plan: Basic, channel: Referral, outcome: Stayed, users: 60}
    - {plan: Basic, channel: Referral, outcome: Left, users: 10}
    - {plan: Pro, channel: Search, outcome: Stayed, users: 50}
    - {plan: Pro, channel: Search, outcome: Left, users: 10}
    - {plan: Pro, channel: Ads, outcome: Stayed, users: 20}
    - {plan: Pro, channel: Ads, outcome: Left, users: 15}
    - {plan: Pro, channel: Referral, outcome: Stayed, users: 45}
    - {plan: Pro, channel: Referral, outcome: Left, users: 5}
encoding: {dimensions: [plan, channel, outcome], y: users}
axes:
  y: {label: Users}
```

## Source

````rmd
```chart
type: alluvial
title: Referred users stay more often than users who came from ads
subtitle: Users by plan, acquisition channel and outcome, synthetic
data:
  inline:
    - {plan: Free, channel: Search, outcome: Stayed, users: 120}
    - {plan: Free, channel: Search, outcome: Left, users: 180}
    - {plan: Free, channel: Ads, outcome: Stayed, users: 60}
    - {plan: Free, channel: Ads, outcome: Left, users: 140}
    - {plan: Free, channel: Referral, outcome: Stayed, users: 70}
    - {plan: Free, channel: Referral, outcome: Left, users: 30}
    - {plan: Basic, channel: Search, outcome: Stayed, users: 90}
    - {plan: Basic, channel: Search, outcome: Left, users: 40}
    - {plan: Basic, channel: Ads, outcome: Stayed, users: 50}
    - {plan: Basic, channel: Ads, outcome: Left, users: 50}
    - {plan: Basic, channel: Referral, outcome: Stayed, users: 60}
    - {plan: Basic, channel: Referral, outcome: Left, users: 10}
    - {plan: Pro, channel: Search, outcome: Stayed, users: 50}
    - {plan: Pro, channel: Search, outcome: Left, users: 10}
    - {plan: Pro, channel: Ads, outcome: Stayed, users: 20}
    - {plan: Pro, channel: Ads, outcome: Left, users: 15}
    - {plan: Pro, channel: Referral, outcome: Stayed, users: 45}
    - {plan: Pro, channel: Referral, outcome: Left, users: 5}
encoding: {dimensions: [plan, channel, outcome], y: users}
axes:
  y: {label: Users}
```
````

## Fields that matter

- `type: alluvial` with `encoding.dimensions`, the categorical fields in the order of the columns from left to right (at least two), and an optional `encoding.y` that weighs each row; without it each row counts 1
- Each dimension is a column of bars, one per category in the order the rows first name it; a title over the column gives the field
- A ribbon joins a category to a category of the next column, and is split by the category of the first dimension, whose colour it takes; the legend names those categories
- Rows that share the same categories are added up; the sums, shares and the sides of a bar are computed by RichView and marked as derived in the readout
- Rows without a category in every dimension, or with a missing or negative value, are left out and counted; rows with a value of zero are not drawn
- An alluvial diagram takes at most 300 nodes (categories over all dimensions) and 2,000 links in a panel; above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count
- `encoding.facet` makes one diagram per value on a shared scale unless `style.facet_scale` is `free`; names that do not fit beside their bar are left out and listed

## Interactions

- Hover or focus a bar to read its size and its share of the whole.
- Hover or focus a ribbon to read the users on that route, the first category it belongs to, and its share of its source and target.
- Click a legend entry to remove a first category and its routes, and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
