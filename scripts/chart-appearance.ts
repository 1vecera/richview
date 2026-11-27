// Computed paint, not markup, determines whether themed chart labels are readable. Canvas text
// is read from the geometry model (`window.__richview.charts()`); captions, legend entries and
// actions are HTML and read from computed style.
import type { Color, TextItem } from './report-model.ts';

export function chartTextAppearance(root: Element) {
  const painter = new OffscreenCanvas(1, 1).getContext('2d', { willReadFrequently: true })!;
  const rgb = (color: string) => {
    painter.clearRect(0, 0, 1, 1); painter.fillStyle = color; painter.fillRect(0, 0, 1, 1);
    return [...painter.getImageData(0, 0, 1, 1).data].slice(0, 3);
  };
  const channels = (values: number[]) => values.map(value => value / 255).map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  const luminance = (values: number[]) => channels(values).reduce((sum, value, i) => sum + value * [.2126, .7152, .0722][i], 0);
  const model = (color: Color) => color.slice(0, 3).map(value => Math.round(value * 255));
  const contrast = (a: number[], b: number[]) => { const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x); return (hi + .05) / (lo + .05); };
  const visible = (node: Element) => {
    const box = node.getBoundingClientRect();
    return box.width > 0 && box.height > 0 && getComputedStyle(node).display !== 'none';
  };
  const charts = window.__richview!.charts().filter(chart => {
    const frame = root.ownerDocument.getElementById(chart.id)?.querySelector('.rv-chart__frame');
    return frame && visible(frame);
  });
  // HTML text sits on the chart's own surface.
  const html = (selector: string) => [...root.querySelectorAll(selector)].filter(visible).map(node => {
    const figure = node.closest('figure[data-rv-chart]');
    const chart = charts.find(item => item.id === figure?.id);
    // Off a chart, text sits on the page colour; a page background (STY-74) leaves the body transparent.
    const surface = chart ? model(chart.geometry.background) : rgb(getComputedStyle(root.ownerDocument.documentElement).getPropertyValue('--rv-bg'));
    const fill = getComputedStyle(node).color;
    return { text: node.textContent!.trim(), fill, surface: surface.join(','), contrast: contrast(rgb(fill), surface) };
  });
  // Series identity on the canvas is the key and the end-label names; value labels sit on marks.
  const canvas = (roles: string[], keep: (item: TextItem) => boolean = () => true) => charts.flatMap(chart => {
    const surface = model(chart.geometry.background);
    return [...chart.geometry.panels.flatMap(panel => panel.items), ...(chart.geometry.figure_items ?? [])]
      .filter((item): item is TextItem => item.kind === 'text' && roles.includes(item.role) && keep(item))
      .map(item => ({ text: item.text, fill: model(item.color).join(','), surface: surface.join(','), contrast: contrast(model(item.color), surface) }));
  });
  const groups = {
    title: html('.rv-chart__title'),
    axis: canvas(['axis-label', 'axis-title']),
    legend: [...html('.rv-chart__legend-entry:not([aria-pressed="false"])'), ...canvas(['key']), ...canvas(['label'], item => !/^[\d.,%\s$€+\-−]+$/.test(item.text))],
    tools: html('.rv-chart__action'),
  };
  return Object.fromEntries(Object.entries(groups).map(([kind, samples]) => [kind, {
    count: samples.length,
    minimum_contrast: Math.min(...samples.map(sample => sample.contrast)),
    fills: [...new Set(samples.map(sample => sample.fill))],
    worst: samples.sort((a, b) => a.contrast - b.contrast)[0],
  }]));
}
