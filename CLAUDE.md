@AGENTS.md

## Claude Code specifics

- Agents `planner` (Opus, effort high) and `implementer` (Opus, effort medium) live in
  `.claude/agents/`; dispatch them with a fresh context through `/tb-ticket`, never inline.
- The reviewer is Codex, not a Claude agent: `scripts/codex-review.sh` (skill `tb-review`).
- Notion is reached through the `claude.ai Notion` MCP server; if it is not connected, stop and
  tell the user instead of tracking work elsewhere.
