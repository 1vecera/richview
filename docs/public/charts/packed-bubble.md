---
title: Packed bubble chart
description: Packed bubble chart for the parts of a total as circles whose area is their value.
styles: [scientific]
---

Use a packed bubble chart to compare many parts of different size at a glance, when the exact order matters less than the overall picture.

```chart
type: packed-bubble
title: Hosting and the two apps take a third of the spend
subtitle: Yearly spend by item in thousands of euros, illustrative figures
data:
  inline:
    - {item: Cloud hosting, spend: 420, area: Operations}
    - {item: Support, spend: 310, area: Operations}
    - {item: Security, spend: 180, area: Operations}
    - {item: Monitoring, spend: 90, area: Operations}
    - {item: Mobile app, spend: 380, area: Product}
    - {item: Web app, spend: 340, area: Product}
    - {item: Search, spend: 150, area: Product}
    - {item: Design, spend: 120, area: Product}
    - {item: Ads, spend: 260, area: Sales}
    - {item: Events, spend: 200, area: Sales}
    - {item: Partners, spend: 140, area: Sales}
    - {item: Hiring, spend: 230, area: People}
    - {item: Training, spend: 80, area: People}
    - {item: Legal, spend: 110, area: Admin}
    - {item: Audit, spend: 60, area: Admin}
    - {item: Rent, spend: 270, area: Admin}
encoding: {label: item, y: spend, color: area}
axes:
  y: {label: Spend (k EUR)}
```

## Source

````rmd
```chart
type: packed-bubble
title: Hosting and the two apps take a third of the spend
subtitle: Yearly spend by item in thousands of euros, illustrative figures
data:
  inline:
    - {item: Cloud hosting, spend: 420, area: Operations}
    - {item: Support, spend: 310, area: Operations}
    - {item: Security, spend: 180, area: Operations}
    - {item: Monitoring, spend: 90, area: Operations}
    - {item: Mobile app, spend: 380, area: Product}
    - {item: Web app, spend: 340, area: Product}
    - {item: Search, spend: 150, area: Product}
    - {item: Design, spend: 120, area: Product}
    - {item: Ads, spend: 260, area: Sales}
    - {item: Events, spend: 200, area: Sales}
    - {item: Partners, spend: 140, area: Sales}
    - {item: Hiring, spend: 230, area: People}
    - {item: Training, spend: 80, area: People}
    - {item: Legal, spend: 110, area: Admin}
    - {item: Audit, spend: 60, area: Admin}
    - {item: Rent, spend: 270, area: Admin}
encoding: {label: item, y: spend, color: area}
axes:
  y: {label: Spend (k EUR)}
```
````

## Fields that matter

- `type: packed-bubble` with `encoding.label` for the name and `encoding.y` for the value, and optional `encoding.color` for a group
- A circle's area is proportional to its value, on one scale in every panel; the circles are packed deterministically, largest first, so the same data always gives the same picture
- A name is written inside its circle only when it fits; the others are reported and stay in the readouts and the table
- A row without a name, a group or a value above zero is left out and reported; a panel takes at most 200 circles, and above that the chart is not drawn and `check` raises `chart.limit-exceeded` naming the limit and the count; a part under a four-hundredth of the largest radius is too small to draw
- The share in a readout is computed by RichView and marked as derived
- `encoding.facet` makes one packing per value; `axes.y.label` names the value

## Interactions

- Hover or focus a circle to read its name, group, value and share.
- Click a legend entry to remove a group and back.
- Open the Table view, Download PNG or CSV, or Expand the chart.
