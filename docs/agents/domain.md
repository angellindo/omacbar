# Domain docs

How the engineering skills consume this repo's domain documentation. Single-context layout.

## Before exploring, read these

- `GLOSSARY.md` at the repo root.
- `docs/adr/`: ADRs that touch the area you are about to work in.
- `docs/scope.md` and `docs/design/button-standard.md` for anything a user can see.
- `docs/research/` for hardware facts (resolution, USB configs, input devices). Never restate a
  hardware fact without citing its research doc.

If a file does not exist, proceed silently; `/domain-modeling` creates them lazily.

## Use the glossary's vocabulary

Name things with `GLOSSARY.md` terms (Layer, Chord, Group, Span, Scene, Frame, Present). Do not drift
to the `_Avoid_` synonyms. A missing concept is a signal to add it via `/domain-modeling`.

## Flag ADR conflicts

> _Contradicts ADR-0002 (display backend is a seam), but worth reopening because…_
