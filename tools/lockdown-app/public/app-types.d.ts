type UiAnswer = import("../types.mts").SavedAnswer;
type UiAnswerInput = import("../types.mts").AnswerInput;
type UiDecision = import("../types.mts").Decision;
type UiImportance = import("../types.mts").Importance;
type UiDecisionCounts = import("../types.mts").DecisionCounts;
type UiCounts = import("../types.mts").Counts;
type UiOption = import("../types.mts").Option;
type UiPartHeader = import("../types.mts").PartHeader;
type UiPartSummary = import("../types.mts").PartSummary;
type UiPartList = import("../types.mts").PartList;
type UiTotals = import("../types.mts").Totals;
type UiDocEntry = import("../types.mts").GuideDocument;
type UiReviewPart = import("../types.mts").ReviewPart;
type UiCard = import("../types.mts").ReviewCard;
type UiItem = import("../types.mts").CrossPartCard & { viewGroup?: string; viewRank?: number };
type UiCardList = { parts: UiPartHeader[]; items: UiItem[] };
type UiAnswerResult = { answer: UiAnswer | null };

type UiPart =
	| (Omit<UiReviewPart, "items"> & { items: UiItem[]; virtual?: undefined })
	| (Pick<UiPartHeader, "id" | "title" | "revision"> &
			Partial<Omit<UiReviewPart, "id" | "title" | "revision" | "items">> &
			{ items: UiItem[]; virtual: "key" | "open" });

type UiFilter = ReturnType<typeof newFilter>;
type UiView = "start" | "part" | "doc" | "answered" | "technical";
type UiRoute =
	| { view: "part"; partId: string; itemId: string | null }
	| { view: "doc"; path: string; anchor?: string | null }
	| { view: "start" | "answered" | "technical" };
type UiDoc = { path: string; loading?: boolean; source?: string; revision?: string | null; error?: string; missing?: boolean };
type UiRepo = ReturnType<typeof repoSets>;
type UiPaths = { paths?: string[]; lockdownPaths?: string[] } | null;
type UiSelection = { decision: UiDecision; changing: number; edits: number; skipped: number };
type UiAnsweredEntry = { id: string; title: string | null; proposed: string | null; partId: string; partTitle: string; answer: UiAnswer; at: string | null };

type UiState = {
	view: UiView | null;
	loaded: boolean;
	parts: UiPartSummary[];
	totals: UiTotals | null;
	listError: string;
	docs: UiDocEntry[];
	docsError: string;
	signature: string;
	part: UiPart | null;
	partId: string | null;
	loadSeq: number;
	loading: boolean;
	pending: Promise<unknown>;
	items: Map<string, UiItem>;
	ui: Map<string, CardUi>;
	filters: Record<"part" | "key" | "open", UiFilter>;
	filter: UiFilter;
	collapse: boolean;
	compact: boolean;
	selection: Set<string>;
	selectAnchor: string | null;
	selectionAsk: UiSelection | null;
	selectionBusy: boolean;
	selectionError: string;
	current: string | null;
	bulk: "idle" | "saving" | "confirm" | "error";
	bulkError: string;
	doc: UiDoc | null;
	docSeq: number;
	refreshTimer: number;
	toastTimer: number;
	itemIndex: Map<string, string>;
	repo: UiRepo | null;
	links: Promise<void> | null;
	linksKey: string;
	firstGroup: { first: string; count: number; answered: number } | null;
	firstGroupKey: string;
	keyStats: (UiCounts & { byPart: Map<string, UiCounts> }) | null;
	keyStatsKey: string;
	refreshAnswers?: (() => void) | null;
	retrying?: boolean;
};

interface CardUi {
	chain: Promise<void>;
	seq: number;
	status: "idle" | "saving" | "saved" | "error" | "conflict";
	error: string;
	intent: UiAnswerInput | null;
	conflict?: JournalIntent | null;
	conflictBase?: string | null;
	expanded: boolean;
	open: boolean;
	editing: boolean;
	noteOpen: boolean;
	editTimer?: number;
	noteTimer?: number;
	editDraft?: string;
	noteDraft?: string;
	editInitial?: string;
	editSnapshot?: UiAnswer | null;
	editDirty?: boolean;
	search?: string;
	card: HTMLElement;
	buttons: Record<UiDecision, HTMLButtonElement>;
	rowButtons: Record<UiDecision, HTMLButtonElement>;
	componentChips: HTMLButtonElement[];
	stateLabel: HTMLElement;
	saveLabel: HTMLElement;
	toggle: HTMLButtonElement;
	staleNote: HTMLElement;
	history: HTMLDetailsElement | null;
	versionBody: HTMLElement;
	versionLabel: HTMLElement;
	version: HTMLElement;
	versionSource?: string;
	excerptSource?: string;
	excerptKind?: string;
	recommended: number;
	editArea: HTMLTextAreaElement;
	editHint: HTMLElement;
	editor: HTMLElement;
	noteArea: HTMLTextAreaElement;
	noteHint: HTMLElement;
	note: HTMLElement;
	noteButton: HTMLButtonElement;
	clearButton: HTMLButtonElement;
	errorText: HTMLElement;
	errorLine: HTMLElement;
	conflictText: HTMLElement;
	conflictLine: HTMLElement;
	select: HTMLInputElement;
	rowTitle: HTMLButtonElement;
	rowExcerpt: HTMLElement;
	rowState: HTMLElement;
	rowStale: HTMLElement;
	rowSave: HTMLElement;
	rowRetry: HTMLButtonElement;
	options?: { row: HTMLLIElement; mark: HTMLSpanElement; button: HTMLButtonElement }[];
}

type UiChild = Node | string | null | undefined | false;
type UiProps = Record<string, string | number | boolean | Record<string, string> | null | undefined>;
type UiHeading = { level: number; text: string; slug: string };
type UiMarkdown =
	| { doc: false; docPath: null; base: string; slugs: null; headings: null }
	| { doc: true; docPath: string; base: string; slugs: Map<string, number>; headings: UiHeading[] };
type UiLink = { href: string; external?: boolean; app?: boolean };
