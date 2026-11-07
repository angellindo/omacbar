# Tickets live in Notion; each pipeline stage has a fixed model

Every ticket is a row in the Notion database "Touchbar Customizer — Tickets" (see
`docs/agents/issue-tracker.md`). Each ticket is **planned by Claude Opus at high effort**,
**implemented by Claude Opus at medium effort**, and **reviewed by Codex `gpt-6.1-sol` at high
effort**, a different model family so the review is not anchored on the author's reasoning.
Routing is fixed in agent frontmatter and `scripts/codex-review.sh`, never chosen per ticket, so
quality is comparable across tickets. Plans and review outputs are mirrored into
`.scratch/TB-<n>/` because Codex reads the repo, not Notion.
