// Historical reproduce scripts read the Wasm renderer and image receipts of a recorded checkout.
export interface RecordedRenderer {
  render_source(source: string, options: string): string;
}

export interface CapturedRender {
  html: string;
  metadata?: { _richview?: { chart_diagnostics?: unknown } };
}

export type RecordedBrowser = Pick<typeof import('playwright'), 'chromium'>;
export type RecordedServer = Pick<typeof import('../../../scripts/browser-harness.ts'), 'serveStatic'>;

export interface CapturePair {
  id: string;
  width: number;
  source: string;
  source_sha256: string;
  theme: string;
  motion?: 'reduce' | 'no-preference';
  before: { file: string; sha256: string };
  after: { file: string; sha256: string };
}

export interface CaptureReceipt {
  before_wasm_sha256: string;
  after_wasm_sha256: string;
  before_node_wasm_sha256?: string;
  after_node_wasm_sha256?: string;
  pairs: CapturePair[];
}

export interface ViewportCaptureReceipt {
  pairs: {
    id: string;
    viewport_css_px: { width: number };
    fixture: string;
    theme: string;
    after: { image: string };
  }[];
}

export interface RecordedCaptureCase {
  source: string;
  width: number;
  theme: string;
  fixture: string;
  name: string;
  motion?: 'reduce' | 'no-preference';
  style?: unknown;
}

export interface PieFixture { id: string; source: string }
export interface PieChart {
  encoding: { x: string; color?: string; facet?: string };
  data: { inline: Record<string, string | number | null>[] };
}
export interface GeneratedReport {
  output: string;
  diagnostics: { level: string }[];
}
