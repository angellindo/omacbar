"""A compile error about the case's new item counts as red, qualified or not.

Pure: `classify(cases, out, compiled)` from the current `scripts/pipeline-gate.py`, fed realistic
rustc diagnostics. No cargo runs.
"""

from __future__ import annotations

import importlib.util
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / "scripts" / "pipeline-gate.py"
CONTRACT_DOC = ROOT / "docs" / "agents" / "contract.md"
DOCUMENTED_CODES = {"E0412", "E0422", "E0425", "E0432", "E0433", "E0560", "E0599", "E0609"}


def load_gate():
    spec = importlib.util.spec_from_file_location("pipeline_gate", GATE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def rustc(header: str) -> str:
    """A cargo failure as rustc prints it: header, location, a label line, the summary."""
    return (
        "   Compiling tb-core v0.1.0 (/repo/crates/tb-core)\n"
        f"{header}\n"
        " --> crates/tb-core/tests/layout_spans.rs:3:5\n"
        "  |\n"
        "3 | use tb_core::Width;\n"
        "  |     ^^^^^^^^^^^^^^ not found\n"
        "\n"
        "error: could not compile `tb-core` (test \"layout_spans\") due to 1 previous error\n"
    )


class MissingSymbolRed(unittest.TestCase):
    def setUp(self):
        self.classify = load_gate().classify

    def status(self, out: str, symbol: str | None, compiled: bool = False) -> str:
        case = {"id": "QA-01", "test": "a_span_two_button_gets_twice_the_free_width", "red": "required"}
        if symbol is not None:
            case["symbol"] = symbol
        return self.classify([case], out, compiled)["QA-01"]

    def assert_red(self, header: str, symbol: str) -> None:
        self.assertEqual(self.status(rustc(header), symbol), "RED_MISSING_SYMBOL", header)

    def assert_compile_error(self, header: str, symbol: str | None) -> None:
        self.assertEqual(self.status(rustc(header), symbol), "COMPILE_ERROR", header)

    # --- one per documented code ---

    def test_an_unresolved_qualified_import_of_the_symbol_is_red(self):
        self.assert_red("error[E0432]: unresolved import `tb_core::Width`", "Width")

    def test_an_unresolved_deeply_qualified_import_of_the_symbol_is_red(self):
        self.assert_red("error[E0432]: unresolved import `tb_core::layout::Width`", "Width")

    def test_an_unknown_type_named_by_the_symbol_is_red(self):
        self.assert_red("error[E0412]: cannot find type `Width` in this scope", "Width")

    def test_an_unknown_qualified_type_named_by_the_symbol_is_red(self):
        self.assert_red("error[E0412]: cannot find type `tb_core::Width` in this scope", "Width")

    def test_an_unknown_struct_or_variant_named_by_the_symbol_is_red(self):
        self.assert_red("error[E0422]: cannot find struct, variant or union type `Width` in this scope", "Width")

    def test_an_unknown_function_named_by_the_symbol_is_red(self):
        self.assert_red("error[E0425]: cannot find function `place` in crate `tb_core`", "place")

    def test_an_unknown_qualified_function_named_by_the_symbol_is_red(self):
        self.assert_red("error[E0425]: cannot find function `tb_core::layout::place` in this scope", "place")

    def test_an_undeclared_type_named_by_the_symbol_is_red(self):
        self.assert_red("error[E0433]: failed to resolve: use of undeclared type `Width`", "Width")

    def test_an_unresolved_qualified_path_ending_in_the_symbol_is_red(self):
        self.assert_red("error[E0433]: failed to resolve: use of undeclared type `tb_core::Width`", "Width")

    def test_an_unknown_struct_field_named_by_the_symbol_is_red(self):
        self.assert_red("error[E0560]: struct `Button` has no field named `span`", "span")

    def test_an_unknown_associated_item_named_by_the_symbol_is_red(self):
        self.assert_red(
            "error[E0599]: no function or associated item named `span` found for enum `Width` in the current scope",
            "span",
        )

    def test_an_unknown_field_access_named_by_the_symbol_is_red(self):
        self.assert_red("error[E0609]: no field `span` on type `Button`", "span")

    # --- what must stay COMPILE_ERROR ---

    def test_a_qualified_name_that_only_starts_with_the_symbol_is_a_compile_error(self):
        self.assert_compile_error("error[E0432]: unresolved import `tb_core::Widths`", "Width")

    def test_a_qualified_name_that_only_ends_with_the_symbol_text_is_a_compile_error(self):
        self.assert_compile_error("error[E0432]: unresolved import `tb_core::MyWidth`", "Width")

    def test_an_unqualified_name_containing_the_symbol_is_a_compile_error(self):
        self.assert_compile_error("error[E0412]: cannot find type `MyWidth` in this scope", "Width")

    def test_an_undocumented_error_code_naming_the_symbol_is_a_compile_error(self):
        self.assert_compile_error("error[E0308]: mismatched types: expected `Width`, found `u16`", "Width")

    def test_a_private_qualified_item_is_a_compile_error(self):
        self.assert_compile_error("error[E0603]: struct `tb_core::Width` is private", "Width")

    def test_a_case_without_a_symbol_is_a_compile_error(self):
        self.assert_compile_error("error[E0432]: unresolved import `tb_core::Width`", None)

    # --- unchanged statuses ---

    def test_a_compiled_run_without_the_test_line_is_not_run(self):
        out = "running 0 tests\n\ntest result: ok. 0 passed; 0 failed\n"
        self.assertEqual(self.status(out, "Width", compiled=True), "NOT_RUN")

    def test_a_failed_test_line_is_red_behavior(self):
        out = "running 1 test\ntest a_span_two_button_gets_twice_the_free_width ... FAILED\n"
        self.assertEqual(self.status(out, "Width", compiled=True), "RED_BEHAVIOR")

    def test_a_passing_test_line_is_green(self):
        out = "running 1 test\ntest a_span_two_button_gets_twice_the_free_width ... ok\n"
        self.assertEqual(self.status(out, "Width", compiled=True), "GREEN")


class DocumentedMissingSymbolCodes(unittest.TestCase):
    def test_the_contract_doc_lists_exactly_the_codes_the_gate_accepts(self):
        row = next(l for l in CONTRACT_DOC.read_text().splitlines() if l.startswith("| `RED_MISSING_SYMBOL`"))
        self.assertEqual(set(re.findall(r"E\d{4}", row)), DOCUMENTED_CODES)
        self.assertEqual(set(load_gate().MISSING_SYMBOL_CODES), DOCUMENTED_CODES)

    def test_the_contract_doc_manifest_example_names_a_bare_symbol(self):
        self.assertIn('"symbol": "Width"', CONTRACT_DOC.read_text())


if __name__ == "__main__":
    unittest.main()
