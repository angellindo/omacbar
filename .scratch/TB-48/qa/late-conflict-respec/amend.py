#!/usr/bin/env python3
"""Late-conflict amendment of TB-48 slices 1-4 (QA). Idempotent: starts from the archived
manifests in .scratch/TB-48/qa/late-conflict-respec/archive/ and writes contract/slice-N.json.

- Every case that was `required` becomes `preserve`, citing the seal and base it was seen red on
  (the runs stay in slice-N.evidence.json; the manifest, seal and evidence are archived with
  SHA256SUMS). Original `preserve` cases and their reasons are unchanged.
- New `required` cases: the late-conflict suites (library, configurator, terminal).
- New frozen files: the suites, the Rust driver and the supervisor.
- The reason keeps its text, with the sentence the amendment supersedes rewritten and one
  amendment sentence appended."""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
ARCHIVE = ROOT / ".scratch/TB-48/qa/late-conflict-respec/archive"
CONTRACT = ROOT / ".scratch/TB-48/contract"

SUPPORT = ["crates/tb-config/tests/support/late_conflict.rs", "crates/tb-config/tests/support/syscall_pause.py"]
LIB = "crates/tb-config/tests/config_late_conflict.rs"
GUI = "crates/tb-app/tests/configurator_late_conflict.rs"
CLI = "crates/tb-config/tests/recover_terminal_late_conflict.rs"
GUI_SUPPORT = ["crates/tb-app/tests/support/configurator.rs", "crates/tb-app/tests/support/ui.rs"]
LIB_SUPPORT = ["crates/tb-config/tests/support/generations.rs", "crates/tb-config/tests/support/recovery.rs"]

RED_BASE = {1: "f3d8ec2", 2: "f14be7c", 3: "5889053", 4: "fd7bc50"}


def lib(test):
    return {"package": "tb-config", "target": "test:config_late_conflict", "test": test}


def gui(test):
    return {"package": "tb-app", "target": "test:configurator_late_conflict", "test": test}


def cli(test):
    return {"package": "tb-config", "target": "test:recover_terminal_late_conflict", "test": test}


NEW = {
    1: [
        lib("a_change_before_the_save_publishes_its_generation_keeps_nothing"),
        lib("a_change_after_the_save_published_its_generation_is_refused_and_the_generation_retained"),
        lib("a_change_after_an_overwrite_published_its_generation_is_refused_and_the_generation_retained"),
        lib("a_config_made_a_symlink_after_the_save_published_its_generation_is_refused_with_the_generation"),
        gui("a_change_before_a_save_publishes_its_generation_keeps_nothing_and_keeps_the_draft"),
        gui("a_change_after_a_save_published_its_generation_names_it_and_keeps_the_draft"),
        gui("a_change_after_an_overwrite_published_its_generation_names_it_and_keeps_the_draft"),
        gui("a_config_made_a_symlink_after_a_save_published_its_generation_is_refused_with_the_id"),
    ],
    2: [
        lib("a_change_after_a_restore_published_its_generation_is_refused_and_the_generation_retained"),
        lib("a_change_after_a_file_removal_published_its_generation_is_refused_and_the_generation_retained"),
        cli("a_change_before_the_restore_publishes_its_generation_is_a_conflict_that_keeps_nothing"),
        cli("a_change_after_the_restore_published_its_generation_exits_3_with_the_id_and_the_retry"),
        cli("a_change_after_a_file_removal_published_its_generation_exits_3_with_the_id_and_the_retry"),
        cli("a_config_made_a_symlink_after_the_restore_published_its_generation_is_refused_with_the_id"),
    ],
    3: [
        lib("a_change_after_a_factory_reset_save_published_its_generation_is_refused_and_the_generation_retained"),
        gui("a_change_after_a_factory_reset_save_published_its_generation_keeps_the_reset_cancellable"),
    ],
    4: [
        gui("a_change_after_a_restore_published_its_generation_names_it_and_shows_the_newer_file"),
        gui("a_config_made_a_symlink_after_a_restore_published_its_generation_is_refused_with_the_id"),
    ],
}

FILES = {
    1: [LIB, GUI, *SUPPORT, *LIB_SUPPORT, *GUI_SUPPORT],
    2: [LIB, CLI, *SUPPORT, *LIB_SUPPORT],
    3: [LIB, GUI, *SUPPORT, *LIB_SUPPORT, *GUI_SUPPORT],
    4: [GUI, *SUPPORT, *GUI_SUPPORT],
}

# (sentence superseded, its replacement); each must occur exactly once in the archived reason.
REWRITE = {
    1: ("Reads and refused saves keep nothing;",
        "Reads, validation failures and conflicts found before the generation is published keep "
        "nothing;"),
    2: ("A file changed while the restore waits is a conflict: exit 3, nothing written or kept, both "
        "versions stay, a later run succeeds.",
        "A file changed while the restore waits, or before its generation is published, is a "
        "conflict: exit 3, nothing written or kept, both versions stay, a later run succeeds."),
    3: ("a save refused as a conflict writes and keeps nothing, keeps the draft and still offers cancel.",
        "a save refused as a conflict before its generation is published writes and keeps nothing, "
        "keeps the draft and still offers cancel."),
    4: ("over a file changed or removed on disk it writes and keeps nothing (TB-43 conflict rules).",
        "over a file changed or removed on disk before its generation is published it writes and "
        "keeps nothing (TB-43 conflict rules)."),
}

AMEND = {
    1: " Late-conflict amendment (ADR-0015): when the last check finds a change after the "
       "generation was published, a save or acknowledged overwrite is refused as ChangedAfterKeep "
       "(file changed) or RefusedAfterKeep (path made unsafe) naming the retained id: the other "
       "editor's file or link is not replaced, the retained generation stays verified and listed "
       "holding exactly what the write would have replaced, the attempt is no success, and a retry "
       "from the file on disk succeeds; the configurator keeps the draft and both conflict "
       "resolutions, names the id and lists it, and its Overwrite then succeeds.",
    2: " Late-conflict amendment (ADR-0015): when the last check finds a change after the `recover` "
       "generation was published, restore and --remove-file exit 3 (exit 1 when the path became "
       "unsafe) and stderr names the config, the retained id and the exact command to run again "
       "(with --remove-file when asked); stdout claims no restore; the other editor's file or link "
       "stays, the retained generation is verified, and that command then restores and keeps the "
       "other editor's file. The library recover and recover_removing report ChangedAfterKeep "
       "with the id and a retry after look succeeds.",
    3: " Late-conflict amendment (ADR-0015): a factory reset's save refused after its "
       "`factory-reset` generation was published names the retained id, never says a file was kept "
       "before a saved reset, keeps the reset draft, both conflict resolutions and `reset-cancel`, "
       "which brings back the draft from before the reset and writes nothing; the library "
       "save_resolved_for(FactoryReset) reports ChangedAfterKeep with the id.",
    4: " Late-conflict amendment (ADR-0015): a restore refused after its `recover` generation was "
       "published says not restored, names the retained id and lists it, shows the newer file "
       "(or keeps the link when the path became unsafe), and tapping the restore again restores.",
}


def main() -> None:
    for n in (1, 2, 3, 4):
        manifest = json.loads((ARCHIVE / f"slice-{n}.json").read_text())
        seal = (ARCHIVE / f"slice-{n}.seal").read_text().strip()
        old, new = REWRITE[n]
        assert manifest["reason"].count(old) == 1, (n, old)
        manifest["reason"] = manifest["reason"].replace(old, new) + AMEND[n]
        for case in manifest["cases"]:
            if case["red"] == "required":
                case["red"] = "preserve"
                case["reason"] = (
                    f"Required until the late-conflict amendment: seen red under seal {seal[:15]}... "
                    f"on {RED_BASE[n]} (runs kept in slice-{n}.evidence.json; manifest, seal and "
                    f"evidence archived in qa/late-conflict-respec/archive with SHA256SUMS); the "
                    f"test is unchanged and now guards implemented behaviour.")
        last = max(int(c["id"].split("-")[1]) for c in manifest["cases"])
        for offset, case in enumerate(NEW[n], start=1):
            manifest["cases"].append({"id": f"QA-{last + offset:02d}", **case, "red": "required"})
        for f in FILES[n]:
            if f not in manifest["files"]:
                manifest["files"].append(f)
        (CONTRACT / f"slice-{n}.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")
        print(n, len(manifest["cases"]), "cases;", len(NEW[n]), "new required;", len(manifest["files"]), "files")


if __name__ == "__main__":
    main()
