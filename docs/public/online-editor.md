---
title: Edit a report online
description: Change a hosted report's source in your browser, see the report beside it and save a new version.
styles: [scientific]
---

The online editor changes a report that is already on RichView's hosted app. You edit its Rich Markdown source on the left. The real report shows on the right and follows your edits. When you are done, you save the result as a new version of the same report.

The editor is part of the hosted app, so it needs an account and the right to edit the report. To try the same editing without an account, follow [the walk-through in the playground](https://richview.ai/docs/online-editor#try-the-editing-in-the-playground) below.

## What the editor is

The editor edits the source. It is not a WYSIWYG page designer: you see and change the Rich Markdown that makes the report, and the report beside it shows what that source produces.

It has two panes.

- **Source.** The report's Rich Markdown, with line numbers and highlighting. A strip above it inserts a block at the cursor, such as a chart, a table, a metric or a callout.
- **Report.** The report as readers see it. Your browser draws it with the same code as the [playground](https://richview.ai/docs/playground), so there is no second renderer. The style pills above it switch the style and write your choice into the `styles` line of the source. [Styles and your brand](https://richview.ai/docs/styles) describes the styles.

Drag the divider between the panes to give one of them more room. On a phone only one pane shows at a time. Switch between **Source** and **Report** at the top.

## Open a report in the editor

Sign in to the [hosted app](https://app.richview.ai) and open **Reports**. Select a report's row: it opens in the editor when you may edit it, and on the report's page when you may not. You can also open the report's address with `/edit` at the end: `/reports/<organization>/<report address>/edit`.

Only people who may edit the report see the editor. Someone who can only read it goes to the report page instead.

## Fix problems as you write

A problem shows on its line. A wave marks the token, a dot sits in the gutter and the message follows on the line. The full list also gives the fix RichView suggests, in words. The messages are the ones the command line and your agent get.

The status bar counts the problems. Select the count to open the full list.

An error never empties the report. While a line needs a fix, the report keeps the last version that drew. A banner says so, and **Go to line** takes you to the problem.

| Key | What it does |
| --- | --- |
| Cmd+S or Ctrl+S | Opens **Save version**. |
| F8 | Goes to the next problem. |
| Shift+F8 | Goes to the previous problem. |
| Escape | Closes the open popover. |

## Save a version

Saving never changes the version you opened. It adds a new one. Every earlier version stays in History, which lists the report's versions.

1. Select **Save version**, or press Cmd+S or Ctrl+S.
2. Read the version number. It is the next patch after the version you opened: 0.0.2 after 0.0.1. You can type a different version number.
3. Select **Save version** in the popover.

The editor writes the number into the `version` line of the source, then publishes the source through the same path as `richview publish`. The server checks the source again with its own engine and applies the same limits as publishing. People the report is shared with see the new version at the report's address at once. If the owner pinned the address to an older version, that version keeps showing; [Choose the right link](https://richview.ai/docs/publishing#choose-the-right-link) explains pins.

If someone saved a newer version after you opened the editor, Save tells you. You can save on top, which makes your text the next version after the newest one, or reload to open the newest version. Nothing is merged and nothing is overwritten. The other person's version stays in History either way.

## Your draft stays in this browser

Until you save, your changes are a draft. The editor keeps it in this browser's local storage, for you, this report and the version you opened; a report you have not saved yet has its own draft, apart from every saved report's. The server never sees it, and nobody who signs in to this browser as someone else is offered it: the editor deletes every other person's drafts when it opens, and signing out clears the browser's storage for the app. The status bar says **Draft kept in this browser**.

When you open the editor again, it offers the kept draft. Signing out, clearing the browser's site data, or opening the editor in another browser or as another person, does not bring a draft back. Save a version first if the work matters.

A report you have not saved yet is created by its first save, and only created: if the address you chose belongs to a report already, or belonged to one that was deleted, the save is refused and nothing is published into it.

## Start a new report

**New report** on the **Reports** page opens the editor on a starter. Nothing is stored until your first save.

The first save creates the report, private by default. Choose who else can open it later with **Share**. The report's address comes from the title. The editor writes the address into the `slug` line, so changing the title later keeps the address. Creating a report needs permission to publish. Saving a version of an existing report needs the right to edit it.

## What the editor does not do

- It edits the source only. The report's data and media files are those of the stored version, and a new version carries them unchanged. To add or replace a file, use [`richview publish`](https://richview.ai/docs/publishing#publish-a-folder-with-data-and-pictures) from your folder. Uploading an image from the editor is not available yet.
- It is not real-time collaboration. Two people editing one report do not see each other's changes. The version rule above settles who saves first.
- It has no offline mode. Opening and saving need the hosted app.
- A save follows the same limits as publishing. A source over the limit shows as a problem in the editor, and a save the server refuses keeps your draft. [Report input limits](https://richview.ai/docs/data#report-input-limits) gives the counting rules.

Slide decks will open in the same editor once the slide format lands, with the preview showing the slide under your cursor. [Slides](https://richview.ai/docs/slides) describes the current format.

## Try the editing in the playground

The [playground](https://richview.ai/playground) needs no account. It uses the same renderer as the editor, so its report matches what the editor shows. It keeps nothing on a server and has no Save version. In the playground a style pill changes only the preview, and the `styles` line stays as you wrote it.

1. Open the playground. The example report is in the left pane, and the report is on the right.
2. In the source, change the `title` line. The title in the report changes as you type.
3. Choose a style pill. The report changes style while the source stays the same.
4. In the `chart` block, change `y: revenue` to `y: revenu`. The diagnostics under the source name the problem and the column you probably meant. The report keeps showing the last version that drew.
5. Change `revenu` back to `revenue`. The diagnostics clear and the report updates.
6. Select **Copy source** or **Download .rmd** to keep your copy. The playground does not store it.

To use the editor on a real report, [open the hosted app](https://app.richview.ai) and sign in. If you have no account, [check whether sign-up is open](https://app.richview.ai/sign-up). [Publish: walkthrough](https://richview.ai/docs/publishing) shows how to get a report there first.
