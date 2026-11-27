// The browser evidence reads the JSON of richview-runtime's models() and richview-chart's Geometry.
export type Point = [number, number];
export type Color = [number, number, number, number];
export type Rect = { x: number; y: number; w: number; h: number };
export type PathCommand = { c: 'm'; p: Point } | { c: 'l'; p: Point } | { c: 'c'; c1: Point; c2: Point; p: Point } | { c: 'z'; p?: never };

interface Fill { color: Color; hatch?: string; hatch_ink?: Color; hatch_scale?: number }
interface Stroke { color: Color; width: number; cap: 'butt' | 'round' | 'square'; dash?: number[] }
interface ItemRole { role: string; series?: number }
interface ObservationItem extends ItemRole { obs?: number; text?: never }

export interface TextItem extends ItemRole {
  kind: 'text';
  obs?: never;
  text: string;
  x: number;
  y: number;
  anchor: 'start' | 'middle' | 'end';
  face: 'data' | 'figures' | 'ui';
  size: number;
  weight: number;
  color: Color;
  rotate?: number;
  bounds: Rect;
  mark?: string;
}

type Path = { cmds: PathCommand[] };
export type Item = TextItem | (ObservationItem & (
  { kind: 'rect'; rect: Rect; fill: Fill; stroke?: Stroke; radius: [number, number, number, number] }
  | { kind: 'line'; path: Path; stroke: Stroke }
  | { kind: 'area'; path: Path; fill: Fill; stroke?: never }
  | { kind: 'point'; center: Point; radius: number; shape: string; fill?: Fill; stroke?: Stroke }
));

type ReadoutLine = { name: string; value: string; series?: number; names_series?: boolean; derived?: boolean };
export type Observation = {
  row?: number;
  dots?: { at: Point; series: number }[];
  series?: number;
  at: Point;
  area?: Rect;
  wedge?: { center: Point; radius: number; inner: number; start: number; end: number };
  radius: number;
  x: number;
  readout: Readout;
};

export type Readout = { heading: string; lines: ReadoutLine[] };
export type Target = { panel: number; observation: number };
export type Targeting = 'nearest-x' | 'radius' | 'contain';

type Span = { domain: Point; extent: Point; range: Point };
type Panel = {
  facet?: string;
  frame: Rect;
  plot: Rect;
  items: Item[];
  observations: Observation[];
  targeting: Targeting;
  x?: Span & { calendar: boolean; y?: Span; least?: number };
  failure?: string;
};

export interface Geometry {
  kind: string;
  width: number;
  height: number;
  background: Color;
  series: { key: string; name: string; slot: number; color: Color; hatch?: string; shape?: string; dash?: number[]; hidden: boolean }[];
  legend?: { swatch: 'square' | 'line' | 'dot'; entries: number[]; bottom: boolean };
  panels: Panel[];
  figure_items?: Item[];
  note?: string;
  summary: string;
  checks: { kind: string; level: string; message: string; fix?: string; path?: string; region?: Rect; facet?: string; details?: unknown }[];
  dropped: { text: string; reason: 'collision' | 'out-of-bounds' | 'density'; kind?: string; wouldBeAt?: Rect; observation?: Point; series?: number }[];
  entrance?: 'sweep' | 'grow' | 'fan' | 'settle' | 'fade';
  failure?: string;
  derived?: unknown[];
}

export interface ChartModel {
  id: string;
  figure: string;
  width: number;
  canvas: { width: number; height: number; dpr: number };
  hidden: string[];
  zoom: Point | null;
  view: Point | null;
  seenView: Point | null;
  target: Target | null;
  pinned: boolean;
  /** What the chart emphasises now: `dim` steps the rest back, `outline` draws the series' dark outline and dims nothing (FEA-CH-52). */
  emphasis: { how: 'dim' | 'outline'; series: number | null } | null;
  settled: boolean;
  state: string;
  playing: boolean;
  expanded: boolean;
  geometry: Geometry;
}

export interface ReportHook {
  output: string;
  settled(): Promise<void>;
  pending(): string[];
  charts(): ChartModel[];
}
