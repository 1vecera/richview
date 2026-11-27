// The persisted review packet and the in-progress captures that feed it.
import type { chartMotion, Pixels, reducedMotion } from './filmstrip.ts';
import type { buildFacts, discoverThemes, produce, ReviewCase, ReviewViewport } from './outputs.ts';
import type { deviceFacts, pace } from './pacing.ts';
import type { Review } from './probes.ts';
import type { focusRelease, mountContinuity, tap } from './runtime.ts';

type Result<T extends (...args: never[]) => unknown> = Awaited<ReturnType<T>>;
type GlyphCheck = ReturnType<Review['glyphs']>[string][number];
export type ReviewChart = ReturnType<Review['charts']>[number] & {
  glyph_checks?: GlyphCheck[] | null;
  box?: { width: number; height: number };
  crop?: string;
  reduced_motion?: Omit<Result<typeof reducedMotion>['charts'][string], 'settled'>;
  motion_error?: string;
  pointer?: Result<typeof chartMotion>['target'];
  entered_while_waiting?: boolean;
  settles_to_reduced?: Pixels & { matches: boolean };
  filmstrip?: string;
};
export type Interaction = { chart: string; type: string } & (
  ({ probe: 'tap' } & Result<typeof tap>) |
  ({ probe: 'focus-release' } & Result<typeof focusRelease>)
);
export interface Summary {
  glyph_findings: number;
  unmeasured_charts: number;
  text_overlaps: number;
  text_clipping: number;
  scroll_hidden_text: number;
  overflow_px: number;
  cls: number;
  console_errors: number;
  reduced_motion_failures: number;
  settle_mismatches: number;
  mount_dh: number | null;
  settled_ms: number;
  runtime_ready: boolean;
  settled_dh: number | null;
  interaction_failures: number | null;
  native_warnings: number;
}
type Output = ReturnType<typeof produce> & { error?: never };
export type CaptureOutput = Output | { error: string; url?: never; native?: never };
export interface Capture {
  theme: string;
  width: number;
  url?: string;
  native?: Output['native'];
  timing_s?: Record<string, number>;
  error?: string;
  charts?: ReviewChart[];
  title?: string;
  page?: string;
  motion_viewport?: { width: number; height: number };
  diagnostics?: Omit<ReturnType<Review['layout']>, 'page'> & { ready: Result<Review['settle']>; console: string[] };
  reduced_motion_page_top?: Pixels;
  mount?: Result<typeof mountContinuity>;
  interaction?: Interaction[];
  summary?: Summary;
}
export type PaceEntry = { theme: string; width: number; chart: string; type: string; throttle: number } &
  Partial<Result<typeof pace>> & { error?: string };
export type CapturedCase = ReviewCase & { captures: Capture[]; pacing: PaceEntry[]; sheet?: string };
export interface ReviewManifest {
  schema: string;
  captured_at: string;
  duration_s: number;
  build: ReturnType<typeof buildFacts>;
  device: Result<typeof deviceFacts> & { load_average_after_pacing?: number[] };
  options: { quick: boolean; jobs: number; cases: string[]; themes: string[]; widths: number[]; baseline: string | null };
  themes: Result<typeof discoverThemes>;
  viewports: ReviewViewport[];
  timeline: { entrance_ms: number[]; pointer_ms: number[]; settled_ms: number; pacing_entrance_window_ms: number; pacing_pointer_window_ms: number; throttle: number[] };
  cases: CapturedCase[];
  errors: string[];
}
