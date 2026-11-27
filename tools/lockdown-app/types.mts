export type Decision = "accept" | "edit" | "reject" | "defer";
export type Importance = "key" | "important" | "detail";
export type ImportanceLevel = Importance | "unrated";

export interface AnswerInput {
	decision: Decision;
	text?: string;
	note?: string;
}

export interface SavedAnswer extends AnswerInput {
	proposedHash?: string;
	updatedAt?: string;
	stale?: boolean;
}

export type DecisionCounts = Record<Decision, number>;

export interface Counts {
	itemCount: number;
	answeredCount: number;
}

export interface PartCounts extends Counts {
	decisions: DecisionCounts;
}

export interface Option {
	label: string;
	detail: string;
}

export interface Source {
	label: string;
	ref: string;
	date: string;
}

export interface Card {
	id: string;
	kind: string;
	refId: string;
	component: string;
	group: string;
	importance: Importance | "";
	title: string;
	question: string;
	why: string;
	proposed: string;
	options: Option[];
	recommendation: string;
	confidence: string;
	sources: Source[];
	conflicts: string;
	images: string[];
}

export interface PartHeader {
	id: string;
	title: string;
	status: string;
	order: number;
	revision: string | null;
	error: string | null;
}

export interface Part extends PartHeader {
	file: string;
	summary: string;
	items: Card[];
	warnings: string[];
}

export interface ReviewCard extends Card {
	answer: SavedAnswer | null;
}

export interface CrossPartCard extends ReviewCard {
	partId: string;
}

export interface OrphanAnswer extends Record<string, unknown> {
	itemId: string;
}

export interface ReviewPart extends Omit<Part, "items">, PartCounts {
	items: ReviewCard[];
	orphans: OrphanAnswer[];
	answersError: string | null;
	answersOnFile: number;
	staleCount: number;
}

export interface PartSummary extends PartHeader, PartCounts {
	file: string;
	staleCount: number;
	importance: Record<ImportanceLevel, Counts>;
	orphanCount: number;
	openIds?: string[];
	warnings: string[];
	answersError: string | null;
}

export interface Totals extends PartCounts {
	parts: number;
	ready: number;
	importance: Record<ImportanceLevel, Counts>;
}

export interface PartList {
	parts: PartSummary[];
	totals: Totals;
}

export interface CardList {
	parts: PartHeader[];
	items: CrossPartCard[];
}

export interface GuideDocument {
	path: string;
	section: string;
	title: string;
	revision: string;
	size: number;
	updatedAt: string;
}
