type JournalAnswer = import("../types.mts").SavedAnswer;
type JournalAnswerInput = import("../types.mts").AnswerInput;
type JournalStorage = Pick<Storage, "getItem" | "setItem" | "removeItem"> | null;

type JournalDecision = (JournalAnswerInput & { clear?: false }) | { clear: true };
type JournalIntent = JournalDecision & { token: string; base: string | null };
type JournalEntry = {
	partId: string;
	itemId: string;
	intent?: JournalIntent;
	noteDraft?: string;
};
type JournalData = { entries: Record<string, JournalEntry & { at: string }> };

interface LockdownJournalApi {
	KEY: string;
	newToken(): string;
	setIntent(storage: JournalStorage, partId: string, itemId: string, next: JournalAnswerInput | null, base: string | null | undefined, token: string): boolean;
	settle(storage: JournalStorage, partId: string, itemId: string, token: string, updatedAt?: string | null): boolean;
	dropIntent(storage: JournalStorage, partId: string, itemId: string): boolean;
	setNoteDraft(storage: JournalStorage, partId: string, itemId: string, note: string): boolean;
	dropNoteDraft(storage: JournalStorage, partId: string, itemId: string): boolean;
	entries(storage: JournalStorage, partId?: string): JournalEntry[];
	get(storage: JournalStorage, partId: string, itemId: string): JournalEntry | null;
	classify(intent: (JournalDecision & { base: string | null }) | null | undefined, answer: JournalAnswer | null | undefined): "none" | "saved" | "retry" | "conflict";
	counts(storage: JournalStorage): { unsaved: number; notes: number };
}

declare var LockdownJournal: LockdownJournalApi;
