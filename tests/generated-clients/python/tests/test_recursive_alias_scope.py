"""Native contracts for alias-wrapped tagged recursion and module scopes."""

import unittest

from alias_tagged_scope.models.branch import Branch
from alias_tagged_scope.models.envelope import Envelope
from alias_tagged_scope.models.item import item_from_dict, item_to_dict
from alias_tagged_scope.models.leaf import Leaf


WIRE = {
    "value": {
        "kind": "branch",
        "children": [
            {"kind": "leaf", "value": "nested"},
        ],
    }
}


class RecursiveAliasScopeContracts(unittest.TestCase):
    def test_alias_wrapped_tagged_payload_decodes_and_encodes(self):
        value = Envelope.from_dict(WIRE)
        self.assertIsInstance(value.value, Branch)
        self.assertIsInstance(value.value.children[0], Leaf)
        self.assertEqual(value.to_dict(), WIRE)

    def test_schema_name_does_not_shadow_converter_module(self):
        value = item_from_dict(WIRE["value"])
        self.assertIsInstance(value, Branch)
        self.assertIsInstance(value.children[0], Leaf)
        self.assertEqual(item_to_dict(value), WIRE["value"])


if __name__ == "__main__":
    unittest.main()
