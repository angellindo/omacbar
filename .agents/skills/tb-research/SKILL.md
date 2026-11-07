---
name: tb-research
description: "Resolve a Research-type Touch Bar Customizer ticket: investigate primary sources and the live machine, write docs/research/NNNN-<slug>.md with cited claims, post [RESEARCH] comments. Use when a ticket's Type is Research or a plan hits a factual unknown."
---

# Research ticket

Uses Matt Pocock's skill `research` (primary sources, one cited Markdown file); this adds the
project conventions.

1. Restate the ticket's question as one sentence and list what would answer it.
2. Investigate **primary sources** in order: the live machine (sysfs, `/proc/bus/input/devices`,
   `lsmod`, descriptors: quote the command), upstream source code and docs, then community repos
   (name the model they tested on). Every claim gets its source; machine facts beat web claims.
3. Write `docs/research/<next NNNN>-<slug>.md`: Status line with the ticket key, Question,
   Findings (tables over prose), Decision input, Sources. Mark untested claims "unverified".
4. Post one `[RESEARCH]` comment per key finding (one line, source, confidence), then move the
   ticket to `In Review` for the Codex check of claims and sources.
   Done when every finding in the doc has a source and a confidence.
5. Follow-ups (spikes, risks) become new tickets via skill `tb-notion` with `Blocked by` edges.
