#!/usr/bin/env python3
"""Bounded LSP assertion-helper regressions; not a replacement for a real server."""
import unittest

from test_rust_analyzer import compact, expansion_code


class ExpansionCodeTests(unittest.TestCase):
    def test_pretty_printing_variants_resolve_the_same_generated_imports(self):
        for use in ["use ::std::string::String", "use::std::string::String",
                    "use\n:: std :: string :: String", "use\r\n::std::string::String"]:
            with self.subTest(use=use):
                aliases = {}
                code = expansion_code(f"{use} as CheckmateAlias;\nstruct User {{ name: CheckmateAlias }}", aliases)
                self.assertEqual(aliases, {"CheckmateAlias": "String"})
                self.assertIn(compact("name: String"), compact(code))

    def test_embedded_text_cannot_supply_an_import_or_expected_field(self):
        aliases = {}
        code = expansion_code(r'const _: &str = "use::std::string::String as Fake; name: String";', aliases)
        self.assertEqual(aliases, {})
        self.assertNotIn(compact("name: String"), compact(code))
        self.assertNotIn("Fake", code)

    def test_aliases_are_shared_for_hover_checks_without_partial_identifier_rewrites(self):
        aliases = {}
        expansion_code("use::core::primitive::bool as CheckmateBool;", aliases)
        code = expansion_code("active: CheckmateBool, unrelated: CheckmateBoolSuffix", aliases)
        self.assertIn("active: bool", code)
        self.assertIn("unrelated: CheckmateBoolSuffix", code)
        self.assertEqual(aliases, {"CheckmateBool": "bool"})

    def test_non_imports_and_wrong_types_do_not_satisfy_the_expected_field(self):
        aliases = {}
        code = expansion_code("abuse::std::string::String as Fake; name: Fake", aliases)
        self.assertEqual(aliases, {})
        self.assertNotIn(compact("name: String"), compact(code))
        code = expansion_code("use::core::primitive::u8 as Alias; name: Alias", aliases)
        self.assertNotIn(compact("name: String"), compact(code))


if __name__ == "__main__":
    unittest.main()
