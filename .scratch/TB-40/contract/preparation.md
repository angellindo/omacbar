# Contract preparation checkpoint
Execution status: **SEALED / BASELINE RED** after orchestrator kernel release at 2026-11-12 08:15 UTC. Frozen tests were formatted, ordinary app/core targets compiled and passed targeted clippy -D warnings, then all four slices were sealed and observed genuinely red against untouched production baseline 8eee25d. The isolated save-clock target fails only for the documented missing setter. Generated gate evidence records exact statuses and logs; no final PASS is claimed. Production source and existing tests remain untouched.
## Public seams
- `State::open`, `view`, `on_key`, public config/chord/selected; actual Rune Runtime taps, text entry, modifier/key events, drags, vertical and horizontal wheel input.
- Runtime `elements` rect/visible/focused/text/color; public `accessibility_tree` author_id/is_disabled for actual disabled controls (ElementInfo.inert does not mean El::disabled).
- `tb_core::config::{validate,parse_valid,warnings}`, real layout through renderer and direct public ConfigFile load/save with byte comparisons.
- Planned `State::set_save_clock(fn() -> Option<u64>)` is isolated in `configurator_save_clock` target, symbol `set_save_clock`. This is the sole intentional missing-symbol compile category.
## Contract files and cases
### Slice 1

Manifest: `contract/slice-1.json`. Required 7, preserve 2.

- QA-101 [required] `the_shell_matches_the_three_columns_and_half_size_footer_canvas`
- QA-102 [required] `all_six_layers_are_navigable_and_visiting_an_absent_layer_does_not_create_it`
- QA-103 [required] `header_counts_and_resolved_widths_come_from_the_shown_grouped_layer`
- QA-104 [required] `selecting_cards_and_preview_keeps_the_edit_panel_and_action_description_together`
- QA-105 [preserve] `modifiers_never_switch_the_manually_selected_layer_or_button`
- QA-106 [preserve] `the_preview_keeps_the_shared_default_and_fn_device_placements`
- QA-107 [required] `short_and_narrow_windows_reach_and_operate_fields_groups_and_save_by_real_scrolling`
- QA-108 [required] `long_group_content_scrolls_to_the_last_card_without_overlapping_the_edit_panel`
- QA-109 [required] `shell_ink_and_background_follow_two_real_omarchy_themes`
### Slice 2

Manifest: `contract/slice-2.json`. Required 7, preserve 1.

- QA-201 [required] `a_group_add_targets_its_own_group_independently_of_selection_and_allocates_free_ids`
- QA-202 [required] `an_explicit_loaded_empty_group_can_receive_its_first_button`
- QA-203 [required] `add_group_on_an_absent_layer_creates_exactly_one_group_and_selects_its_none_button`
- QA-204 [required] `card_drag_moves_before_and_after_targets_in_other_groups_without_losing_selection`
- QA-205 [required] `card_drag_reorders_inside_a_group_and_cancel_or_self_drop_changes_nothing`
- QA-206 [required] `selecting_a_card_blurs_text_and_alt_arrows_move_across_groups_with_the_draft_attached`
- QA-207 [required] `removing_the_only_button_discards_its_issue_and_reindexes_group_add_targets`
- QA-208 [preserve] `existing_toolbar_add_and_keyboard_step_keep_their_domain_order`
### Slice 3

Manifest: `contract/slice-3.json`. Required 12, preserve 3.

- QA-301 [required] `label_room_uses_resolved_device_width_and_shared_font_measurement`
- QA-302 [required] `width_mode_numeric_fields_edit_span_and_pixels_and_keep_full_string_entry`
- QA-303 [required] `numeric_zero_and_overflow_drafts_are_never_silently_clamped_or_saved_as_old_widths`
- QA-304 [required] `two_buttons_and_two_layers_keep_independent_invalid_width_buffers_until_each_is_repaired`
- QA-305 [required] `key_and_command_switches_keep_separate_text_buffers_and_none_keeps_the_command_memory`
- QA-306 [required] `ordinary_modifier_buttons_cannot_newly_select_key_but_legacy_keys_and_escape_stay_readable`
- QA-307 [preserve] `default_and_fn_still_allow_key_actions_with_shared_unknown_key_diagnostics`
- QA-308 [required] `selected_invalid_cards_and_preview_have_distinct_error_and_accent_decorations`
- QA-309 [preserve] `fill_text_and_border_diagnostics_and_hex_notices_keep_the_shared_theme_semantics`
- QA-310 [required] `all_shared_warning_kinds_are_visible_and_remain_nonblocking`
- QA-311 [required] `empty_command_save_rejection_preserves_the_accepted_config_and_exact_file_bytes`
- QA-312 [required] `command_diagnostics_remain_global_after_switching_to_a_valid_layer`
- QA-313 [required] `empty_and_whitespace_commands_report_each_chord_and_button_in_shared_validation`
- QA-314 [required] `version_one_parse_rejects_empty_commands_and_accepts_none_without_executing_any_action`
- QA-315 [preserve] `legacy_modifier_keys_remain_valid_and_report_nonblocking_shared_warnings`
### Slice 4

Manifest: `contract/slice-4.json`. Required 11, preserve 1.

- QA-401 [required] `edit_then_revert_clears_dirty_and_marks_only_the_layers_that_still_differ`
- QA-402 [required] `reversed_reorder_and_add_remove_restore_baseline_including_absent_optional_layers`
- QA-403 [required] `an_originally_present_empty_layer_keeps_its_presence_and_explicit_gap_after_add_remove`
- QA-404 [required] `invalid_width_alone_is_dirty_but_equivalent_width_spelling_and_inactive_action_memory_are_clean`
- QA-405 [required] `another_layers_invalid_draft_blocks_the_actual_save_and_focused_save_shortcut_until_repaired`
- QA-406 [required] `save_shortcut_works_from_each_field_without_swallowing_ordinary_text_or_select_all`
- QA-407 [required] `successful_save_updates_the_baseline_and_reports_only_file_state_and_no_acknowledgement`
- QA-408 [required] `deterministic_write_failure_preserves_edits_baseline_and_previous_file_bytes`
- QA-409 [required] `invalid_startup_and_missing_home_explain_the_unsaved_replacement_and_unavailable_save`
- QA-410 [required] `whole_config_changes_remain_dirty_even_when_no_layer_differs`
- QA-411 [preserve] `warning_only_legacy_keys_round_trip_version_one_with_style_and_explicit_group_gaps`
- QA-412 [required] `only_successful_explicit_saves_sample_the_injected_clock_and_update_the_displayed_utc_time`

## Acceptance coverage

- Acceptance 1: slice 1 literal dividers/footer/preview/card/field coordinates, six rows, actual small-window scrolling and operations, two real Omarchy themes. Portable design reference and inspected snapshots remain post-release implementation/functional QA evidence.
- Acceptance 2: slices 1–2 real header counts and remainder widths, shared selection, chosen-group/empty-group/absent-layer insertion, stable ids/gaps, two-dimensional card drag before/after across groups, cancellation/self-drop and Alt reorder with draft identity. Existing preview/ESC reorder tests remain untouched.
- Acceptance 3: slice 3 real font measurement and label room; width modes/numeric invalid drafts; per-button/layer caches; independent action buffers; modifier policy and readable legacy keys/ESC; theme colors; concurrent editor error/selection and unchanged scene borders.
- Acceptance 4: slices 3–4 shared whitespace-command rejection, parse/save byte preservation, actual disabled accessibility semantics, all-layer draft errors, focused Ctrl+S across every editable kind and warnings-only saves.
- Acceptance 5: slice 4 actual baseline/reverts, optional-layer presence, originally empty layer/gap, dirty invalid active drafts versus inactive memory, write-failure data/baseline/time preservation and isolated deterministic successful-save clock. Slice 1 covers physical modifier selection invariance.
- Acceptance 6: all manifests are independent public behavioral contracts. Existing tests preserved. Scripts, screenshots/image inspection, final gates/review remain after kernel release.

## Pinned Rune source checked read-only

Revision `f73f72cdc563534e4f7de9f5a79bed007ac42384`: `runtime/elements.rs`, `runtime/input/keys.rs`, `runtime/scroll/x.rs`, `edit/keys.rs`, `el/a11y.rs`, `a11y/tree.rs`, `a11y/dump.rs`. `wheel_xy` accepts dx/dy, `Key::Modifier`/Modifier::Fn exist, accessibility_tree nodes expose disabled semantics; support helper reveals by intersecting actual keyed ancestor scroll viewports with the window.

## Resume sequence

1. Orchestrator releases `.scratch/TB-40/kernel-gate.json` after kernel completion. QA never releases it.
2. Format the contract files only, compile each target against untouched baseline (the isolated clock target may fail only for the documented setter). Correct actual harness mistakes before sealing.
3. Stamp each slice, then establish genuine red per required case before applying any candidate production files for that slice. Candidate files remain under `.scratch/TB-40/candidate/` until then.
4. All manifests intentionally freeze the same stable support helper and existing support/ui.rs; format all shared support before the first seal. Any later helper correction requires resealing/re-establishing red for every affected manifest.
