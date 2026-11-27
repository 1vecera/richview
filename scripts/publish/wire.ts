// JSON read from the Rust publication builder's release.json.
export interface ReleaseManifest {
	id: string;
	commit: string;
	objects: Record<string, { bytes: number }>;
	site: { mode: string; layout: string; theme: string };
	items: { audience: string; state: string }[];
}
