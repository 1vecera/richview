// Browser views for the local lockdown dashboard. Cards and Markdown use the app's existing renderer.
"use strict";

const technicalPages = () => state.docs.filter(doc => doc.section === "how-it-works");
const technicalPath = path => path?.startsWith("docs/how-it-works/");
const technicalIndex = "docs/how-it-works/README.md";

function dashboardDoors() {
	const totals = state.totals;
	const orphans = state.parts.reduce((sum, part) => sum + (part.orphanCount || 0), 0);
	const door = (title, href, count, unit) => el("a", { href, class: "dashboard-door" },
		el("span", { class: "door-title" }, title),
		el("span", { class: "door-count" }, count == null ? "Loading…" : plural(count, unit)));
	return el("nav", { class: "dashboard-doors", "aria-label": "Project dashboard" },
		door("How it works", "#/how-it-works", state.loaded ? technicalPages().length : null, "page"),
		door("Answered", "#/answered", totals && totals.answeredCount + orphans, "answer"),
		door("Open questions", "#/open", totals && totals.itemCount - totals.answeredCount, "question"));
}

function technicalPageList() {
	const pages = technicalPages();
	return pages.length ? el("ul", { class: "technical-list" }, pages.map(doc =>
		el("li", {}, el("a", { href: guideHref(doc.path) }, doc.title)))) : null;
}

function showTechnicalIndex() {
	document.title = "How it works · RichView lockdown";
	setChildren(els.page, el("h1", { class: "part-title" }, "How it works"),
		technicalPageList() || el("p", { class: "start-note" }, state.docsError || (state.loaded
			? "Technical pages appear here as they are added to docs/how-it-works/." : "Loading pages…")));
}

function historyDisclosure(item) {
	const body = el("div", { class: "md", role: "status" });
	const details = el("details", { class: "proposal-history" },
		el("summary", {}, "Proposal when answered"), body);
	details.addEventListener("toggle", async () => {
		if (!details.open || body.childNodes.length) return;
		body.textContent = "Looking up the earlier proposal…";
		try {
			const result = await api("GET", `/api/proposal/${encodeURIComponent(item.partId)}/${encodeURIComponent(item.id)}`);
			if (result.proposed === null) body.textContent = "The earlier wording was not stored with the answer and could not be found in local history.";
			else body.innerHTML = renderMarkdown(result.proposed) || "<p>Empty proposal.</p>";
		} catch (error) {
			body.textContent = error.message;
			details.addEventListener("toggle", () => { if (!details.open) body.replaceChildren(); }, { once: true });
		}
	});
	return details;
}

function answerEntry(item) {
	const { answer, at, partId, partTitle, proposed, title, id } = item;
	const href = title === null ? partHref(partId) : `${partHref(partId)}/${encodeURIComponent(id)}`;
	const date = at ? new Date(at) : null;
	const time = date?.toLocaleString(undefined, { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
	return el("article", { class: "answer-entry" },
		el("div", { class: "answer-meta" },
			el("span", { class: `decision-state is-${answer.decision}` }, DECISION_STATE[answer.decision]),
			el("time", { datetime: at }, time || "Date unavailable"),
			el("span", {}, partTitle)),
		el("h2", { class: "card-title" }, el("a", { href }, el("span", { class: "answer-id" }, id), " ", titleNodes(title ?? "Card removed from this part"))),
		answer.text ? block("Your answer", renderMarkdown(answer.text), "version", "serif") : null,
		answer.note ? el("div", { class: "answer-note" }, el("p", { class: "label" }, "Your note"), el("p", {}, answer.note)) : null,
		answer.stale ? el("p", { class: "rail-stale" }, "Proposal changed after your answer.") : null,
		answer.stale ? historyDisclosure(item) : null,
		proposed === null ? null : block(answer.stale ? "Current proposal" : "Proposal when answered",
			renderMarkdown(proposed) || "<p>Empty proposal.</p>", "proposal"));
}

// Keep the controls mounted while new decisions arrive so polling never interrupts a search.
async function openAnswered() {
	const title = el("h1", { class: "part-title" }, "Answered");
	const status = el("p", { class: "start-note", role: "status" }, "Loading answers…");
	const results = el("div", { class: "answer-list" });
	const part = el("select");
	const decision = el("select", {}, el("option", { value: "" }, "All decisions"),
		DECISIONS.map(value => el("option", { value }, DECISION_STATE[value])));
	const search = el("input", { type: "search", placeholder: "Search answers", id: "answered-query" });
	const field = (name, control) => el("label", { class: "field" }, el("span", { class: "field-label" }, name), control);
	const filters = el("div", { class: "toolbar answer-filters", role: "search" },
		field("Search", search), field("Part", part), field("Decision", decision));
	setChildren(els.page, title, filters, status, results);
	document.title = "Answered · RichView lockdown";
	let entries = [];
	const filter = () => {
		let count = 0;
		const query = search.value.trim().toLowerCase();
		for (const { item, card, text } of entries) {
			card.hidden = Boolean((part.value && item.partId !== part.value)
				|| (decision.value && item.answer.decision !== decision.value) || !text.includes(query));
			if (!card.hidden) count++;
		}
		status.textContent = count === entries.length ? plural(count, "answer") : `${count} of ${plural(entries.length, "answer")}`;
	};
	filters.addEventListener("input", filter);
	let signature;
	let loading = false;
	const refreshAnswers = async () => {
		if (loading || !title.isConnected) return;
		loading = true;
		try {
			await state.pending;
			const { items } = await api("GET", "/api/answered");
			if (!title.isConnected) return;
			const next = JSON.stringify(items);
			if (signature === next) return;
			signature = next;
			const choice = part.value;
			const parts = new Map(items.map(item => [item.partId, item.partTitle]));
			setChildren(part, el("option", { value: "" }, "All parts"), [...parts].sort(([a], [b]) => a.localeCompare(b))
				.map(([value, label]) => el("option", { value }, label)));
			part.value = parts.has(choice) ? choice : "";
			entries = items.map(item => ({ item, card: answerEntry(item), text: [
				item.id, item.title, item.partTitle, item.proposed, item.answer.decision, item.answer.text, item.answer.note
			].join(" ").toLowerCase() }));
			results.replaceChildren(...entries.map(entry => entry.card));
			filter();
		} catch (error) {
			signature = undefined;
			status.textContent = `Cannot load answers. ${error.message}`;
		} finally {
			loading = false;
		}
	};
	state.refreshAnswers = refreshAnswers;
	await linksReady();
	await refreshAnswers();
}
