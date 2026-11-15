"""Native consumers of recursive tagged payloads; just/CI owns generation."""

import importlib
import json
import unittest

from recursive_tagged_payloads.models import (
    PlainVariantBranch,
    VariantBranch,
    VariantBranchFields,
    VariantLeaf,
)
from recursive_tagged_payloads.models.plain_variant import (
    plain_variant_from_dict,
    plain_variant_to_dict,
)
from recursive_tagged_payloads.models.variant import variant_from_dict, variant_to_dict
from recursive_tagged_payloads.runtime import Client


WIRE = {
    "kind": "branch",
    "name": "root",
    "children": [
        {"kind": "leaf", "value": "array"},
        {
            "kind": "branch",
            "name": "nested",
            "children": [],
            "optional_child": {"kind": "leaf", "value": "optional"},
        },
    ],
    "optional_child": {
        "kind": "branch",
        "name": "optional branch",
        "children": [{"kind": "leaf", "value": "end"}],
    },
    "mapped_children": {
        "leaf_key": {"kind": "leaf", "value": "map"},
        "branch_key": {"kind": "branch", "name": "mapped branch", "children": []},
    },
}


class RecursiveTaggedPayloadContracts(unittest.TestCase):
    def test_public_runtime_and_model_imports(self):
        self.assertIs(importlib.import_module("recursive_tagged_payloads.runtime").Client, Client)
        for module, name, model in (
            ("variant_branch", "VariantBranch", VariantBranch),
            ("variant_branch_fields", "VariantBranchFields", VariantBranchFields),
            ("variant_leaf", "VariantLeaf", VariantLeaf),
            ("plain_variant_branch", "PlainVariantBranch", PlainVariantBranch),
        ):
            with self.subTest(module=module):
                imported = importlib.import_module(f"recursive_tagged_payloads.models.{module}")
                self.assertIs(getattr(imported, name), model)

    def test_finite_recursive_values_decode_and_encode_losslessly(self):
        for branch, decode, encode in (
            (VariantBranch, variant_from_dict, variant_to_dict),
            (PlainVariantBranch, plain_variant_from_dict, plain_variant_to_dict),
        ):
            with self.subTest(payload=branch.__name__):
                value = decode(WIRE)
                self.assertIsInstance(value, branch)
                self.assertIsInstance(value.children[0], VariantLeaf)
                self.assertIsInstance(value.children[1], branch)
                self.assertIsInstance(value.children[1].optional_child, VariantLeaf)
                self.assertIsInstance(value.optional_child, branch)
                self.assertIsInstance(value.optional_child.children[0], VariantLeaf)
                self.assertIsNone(value.optional_child.optional_child)
                self.assertIsNone(value.optional_child.mapped_children)
                self.assertIsInstance(value.mapped_children["leaf_key"], VariantLeaf)
                self.assertIsInstance(value.mapped_children["branch_key"], branch)
                self.assertEqual(json.loads(json.dumps(encode(value))), WIRE)

    def test_explicit_recursive_instances_encode_without_name_errors(self):
        for branch, encode in (
            (VariantBranch, variant_to_dict),
            (PlainVariantBranch, plain_variant_to_dict),
        ):
            with self.subTest(payload=branch.__name__):
                value = branch(
                    name="root",
                    children=[
                        VariantLeaf(value="array"),
                        branch(
                            name="nested",
                            children=[],
                            optional_child=VariantLeaf(value="optional"),
                        ),
                    ],
                    optional_child=branch(
                        name="optional branch", children=[VariantLeaf(value="end")]
                    ),
                    mapped_children={
                        "leaf_key": VariantLeaf(value="map"),
                        "branch_key": branch(name="mapped branch", children=[]),
                    },
                )
                self.assertEqual(encode(value), WIRE)

    def test_absent_optional_recursive_values_stay_absent(self):
        wire = {"kind": "branch", "name": "empty", "children": []}
        for decode, encode in (
            (variant_from_dict, variant_to_dict),
            (plain_variant_from_dict, plain_variant_to_dict),
        ):
            with self.subTest(decode=decode.__name__):
                value = decode(wire)
                self.assertIsNone(value.optional_child)
                self.assertIsNone(value.mapped_children)
                self.assertEqual(encode(value), wire)

    def test_unknown_tags_raise_value_error_at_recursive_boundaries(self):
        unknown = {"kind": "unknown"}
        for decode in (variant_from_dict, plain_variant_from_dict):
            for position, wire in (
                ("root", unknown),
                ("array", {"kind": "branch", "name": "root", "children": [unknown]}),
                (
                    "optional",
                    {
                        "kind": "branch",
                        "name": "root",
                        "children": [],
                        "optional_child": unknown,
                    },
                ),
                (
                    "map",
                    {
                        "kind": "branch",
                        "name": "root",
                        "children": [],
                        "mapped_children": {"unknown_key": unknown},
                    },
                ),
            ):
                with self.subTest(decode=decode.__name__, position=position):
                    with self.assertRaises(ValueError):
                        decode(wire)

    def test_named_allof_payload_works_without_an_outer_discriminator(self):
        wire = {key: value for key, value in WIRE.items() if key != "kind"}
        value = VariantBranch.from_dict(wire)
        self.assertIsInstance(value.children[1], VariantBranch)
        self.assertIsInstance(value.optional_child, VariantBranch)
        self.assertIsInstance(value.mapped_children["branch_key"], VariantBranch)
        self.assertEqual(value.to_dict(), wire)

    def test_referenced_allof_fields_keep_recursive_helpers(self):
        wire = {
            "children": [{"kind": "leaf", "value": "array"}],
            "optional_child": {"kind": "branch", "name": "empty", "children": []},
            "mapped_children": {"leaf_key": {"kind": "leaf", "value": "map"}},
        }
        value = VariantBranchFields.from_dict(wire)
        self.assertIsInstance(value.children[0], VariantLeaf)
        self.assertIsInstance(value.optional_child, VariantBranch)
        self.assertIsInstance(value.mapped_children["leaf_key"], VariantLeaf)
        self.assertEqual(value.to_dict(), wire)
        explicit = VariantBranchFields(
            children=[VariantLeaf(value="array")],
            optional_child=VariantBranch(name="empty", children=[]),
            mapped_children={"leaf_key": VariantLeaf(value="map")},
        )
        self.assertEqual(explicit.to_dict(), wire)


if __name__ == "__main__":
    unittest.main()
