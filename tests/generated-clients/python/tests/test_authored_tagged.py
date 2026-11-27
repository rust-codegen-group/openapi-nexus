"""Native consumers of authored tagged unions from either Python transport."""

import importlib
import unittest
from unittest.mock import patch

from test_union import sent_json, transport


class AuthoredTaggedContracts(unittest.TestCase):
    def test_optional_and_alternate_discriminator_round_trip(self):
        union = importlib.import_module("authored_four.models.union")
        for wire in (
            {"payload_a": "a", "payload_a_schema": {"keep_key": True}},
            {"kind": "variant_a", "payload_a": "a", "payload_a_schema": {}},
            {"kind": "variant_b_1", "payload_b": 1},
            {"kind": "variant_b_2", "payload_b": 2},
        ):
            with self.subTest(wire=wire):
                value = union.union_from_dict(wire)
                self.assertEqual(union.union_to_dict(value), wire)
                models = importlib.import_module("authored_four.models")
                if "payload_a" in wire:
                    self.assertIsInstance(value, models.UnionVariantACase)
                    self.assertIsInstance(value.payload, models.PayloadA)
                else:
                    self.assertIsInstance(value, models.UnionPayloadBCase)
                    self.assertIsInstance(value.payload, models.PayloadB)
                    self.assertFalse(hasattr(value, "discriminator"))

    def test_presence_ambiguity_and_raw_are_independent(self):
        first = importlib.import_module("authored_default.models.choice")
        strict = importlib.import_module("authored_strict.models.choice")
        raw = importlib.import_module("authored_raw.models.choice")
        overlap = {"label": "shared"}
        self.assertIsInstance(first.choice_from_dict(overlap), first.ChoiceACase)
        with self.assertRaisesRegex(ValueError, "Ambiguous inferred tagged union Choice"):
            strict.choice_from_dict(overlap)
        for codec in (first, strict):
            for wire, case in (
                ({"label": "beta"}, codec.ChoiceBCase),
                ({"label": "beta", "kind": "b_alt"}, codec.ChoiceBCase),
                ({"label": "gamma", "kind": None}, codec.ChoiceCCase),
            ):
                with self.subTest(policy=codec.__package__, wire=wire):
                    value = codec.choice_from_dict(wire)
                    self.assertIsInstance(value, case)
                    if "kind" in wire:
                        self.assertEqual(value.discriminator, wire["kind"])
                    else:
                        self.assertIs(value.discriminator, codec.AbsentDiscriminator.ABSENT)
                    self.assertEqual(codec.choice_to_dict(value), wire)
            for invalid in ({"label": "gamma"}, {"label": "alpha", "kind": "unknown"}):
                with self.assertRaisesRegex(ValueError, "No matching inferred tagged union"):
                    codec.choice_from_dict(invalid)
        for wire in (overlap, None, {"kind": "unknown", "keep_extra_key": [None, 2**62]}):
            self.assertIs(raw.choice_from_dict(wire), wire)
            self.assertIs(raw.choice_to_dict(wire), wire)

    def test_nested_recursive_maps_and_conjunct_local_additional_properties(self):
        env = importlib.import_module("authored_strict.models.envelope")
        tree = importlib.import_module("authored_strict.models.tree")
        wire = {"tree": {"children": {"keep_raw_key": {"label": "leaf", "extra_count": 5}}}}
        value = env.envelope_from_dict(wire)
        self.assertIsInstance(value, env.EnvelopeTreeCase)
        self.assertIsInstance(value.payload.tree, tree.TreeNodeCase)
        child = value.payload.tree.payload.children["keep_raw_key"]
        self.assertIsInstance(child, tree.TreeLeafCase)
        self.assertEqual(child.payload.additional_properties, {"extra_count": 5})
        self.assertEqual(env.envelope_to_dict(value), wire)
        with self.assertRaisesRegex(ValueError, "No matching inferred tagged union"):
            env.envelope_from_dict(
                {"tree": {"children": {"keep_raw_key": {"label": "leaf", "extra_count": True}}}}
            )
        composed = importlib.import_module("authored_strict.models.composed")
        allowed = {"label": "l", "extra_count": 5}
        self.assertEqual(composed.composed_to_dict(composed.composed_from_dict(allowed)), allowed)
        # A declaration in a sibling allOf conjunct does not exempt a key from
        # the dictionary value type of this conjunct.
        with self.assertRaisesRegex(ValueError, "No matching inferred tagged union"):
            composed.composed_from_dict({"kind": "a", "label": "l", "extra_count": 5})

    def test_concrete_precedes_opaque_and_common_shape_applies_to_both(self):
        for policy in ("default", "strict"):
            codec = importlib.import_module(f"authored_{policy}.models.gated_choice")
            concrete = {"scope": "s", "payload_value": "value"}
            opaque = {"scope": "s", "extra_key": 6}
            for wire, case in (
                (concrete, codec.GatedChoiceTypedCase),
                (opaque, codec.GatedChoiceOpaquePayloadCase),
            ):
                value = codec.gated_choice_from_dict(wire)
                self.assertIsInstance(value, case)
                self.assertEqual(codec.gated_choice_to_dict(value), wire)
            with self.assertRaisesRegex(ValueError, "No matching inferred tagged union"):
                codec.gated_choice_from_dict({"payload_value": "value"})

            outer = importlib.import_module(f"authored_{policy}.models.gated_container")
            models = importlib.import_module(f"authored_{policy}.models")
            peer = {"scope": "s", "other_value": "peer"}
            value = outer.gated_container_from_dict(peer)
            self.assertIsInstance(value, models.OtherPayload)
            self.assertEqual(outer.gated_container_to_dict(value), peer)

    def test_selected_decoder_error_does_not_retry_overlapping_members(self):
        codec = importlib.import_module("authored_default.models.choice")
        with (
            patch.object(
                codec.ChoiceA, "from_dict", side_effect=ValueError("Selected payload error")
            ) as first,
            patch.object(codec.ChoiceB, "from_dict", wraps=codec.ChoiceB.from_dict) as second,
        ):
            with self.assertRaisesRegex(ValueError, "Selected payload error"):
                codec.choice_from_dict({"label": "shared"})
            first.assert_called_once_with({"label": "shared"})
            second.assert_not_called()

    def test_case_owned_discriminator_cannot_be_overwritten_by_payload(self):
        codec = importlib.import_module("authored_strict.models.gated_choice")
        value = codec.GatedChoiceTypedCase(
            {"scope": "s", "payload_value": "value", "kind": "wrong"}, "typed"
        )
        with self.assertRaisesRegex(ValueError, "Conflicting inferred discriminator"):
            codec.gated_choice_to_dict(value)

    def test_retained_tag_reaches_nested_codec_and_does_not_hide_its_error(self):
        message = importlib.import_module("authored_echo_strict.models.message")
        nested = importlib.import_module("authored_echo_strict.models.nested_message")
        for wire in (
            {"kind": "text_alt", "label": "l"},
            {"kind": "count", "count": 5, "nullable_count": None},
        ):
            value = nested.nested_message_from_dict(wire)
            self.assertFalse(hasattr(value, "discriminator"))
            self.assertIsInstance(
                value.payload, (message.MessageTextCase, message.MessageCountCase)
            )
            self.assertEqual(nested.nested_message_to_dict(value), wire)
        ambiguous = {"label": "l", "count": 5, "nullable_count": None}
        with self.assertRaisesRegex(ValueError, "Ambiguous inferred tagged union Message:"):
            nested.nested_message_from_dict(ambiguous)
        with self.assertRaisesRegex(ValueError, "No matching inferred tagged union"):
            nested.nested_message_from_dict({"kind": "count", "count": 5})

    def test_nested_required_null_survives_without_rewriting_reusable_models(self):
        codec = importlib.import_module("authored_echo_strict.models.metadata_message")
        models = importlib.import_module("authored_echo_strict.models")
        wire = {"kind": "meta", "detail": {"value": None}}
        value = codec.metadata_message_from_dict(wire)
        self.assertEqual(codec.metadata_message_to_dict(value), wire)
        self.assertEqual(models.Metadata.from_dict({"value": None}).to_dict(), {})

    def test_non_object_branch_keeps_its_whole_wire_value(self):
        codec = importlib.import_module("authored_echo_strict.models.mixed_message")
        value = codec.mixed_message_from_dict("old format")
        self.assertEqual(value.payload, "old format")
        self.assertEqual(codec.mixed_message_to_dict(value), "old format")
        with self.assertRaisesRegex(ValueError, "discriminator"):
            codec.mixed_message_to_dict(type(value)("old format", "label"))
        wire = {"kind": "label", "label": "new format"}
        self.assertEqual(codec.mixed_message_to_dict(codec.mixed_message_from_dict(wire)), wire)

    def test_projected_common_scope_keeps_the_selected_nested_decoder(self):
        codec = importlib.import_module("authored_echo_strict.models.gated_message")
        good = {"scope": "s", "kind": "text_alt", "label": "l"}
        self.assertEqual(codec.gated_message_to_dict(codec.gated_message_from_dict(good)), good)
        bad = {"scope": "s", "label": "l", "count": 5, "nullable_count": None}
        with self.assertRaisesRegex(ValueError, "Ambiguous inferred tagged union Message:"):
            codec.gated_message_from_dict(bad)

    def test_projected_write_checks_the_assembled_value_with_the_selected_codec(self):
        codec = importlib.import_module("authored_echo_strict.models.gated_message")
        value = codec.gated_message_from_dict({"scope": "s", "label": "l"})
        value.payload.update(count=5, nullable_count=None)
        with self.assertRaisesRegex(ValueError, "Ambiguous inferred tagged union Message:"):
            codec.gated_message_to_dict(value)

        value.discriminator = "text_alt"
        self.assertEqual(
            codec.gated_message_to_dict(value),
            {
                "scope": "s",
                "label": "l",
                "count": 5,
                "nullable_count": None,
                "kind": "text_alt",
            },
        )
        self.assertNotIn("kind", value.payload)
        for policy in ("default", "strict"):
            with self.subTest(policy=policy):
                codec = importlib.import_module(f"authored_echo_{policy}.models.gated_message")
                wire = {"scope": "s", "label": "l"}
                self.assertEqual(
                    codec.gated_message_to_dict(codec.gated_message_from_dict(wire)),
                    wire,
                )

    def test_projected_object_write_still_checks_its_nested_child(self):
        codec = importlib.import_module("authored_echo_strict.models.metadata_message")
        wire = {"kind": "meta", "detail": {"value": None}, "child": {"label": "l"}}
        value = codec.metadata_message_from_dict(wire)
        self.assertEqual(codec.metadata_message_to_dict(value), wire)
        value.payload["child"].update(count=5, nullable_count=None)
        with self.assertRaisesRegex(ValueError, "Ambiguous inferred tagged union Message:"):
            codec.metadata_message_to_dict(value)

    def test_optional_raw_fields_keep_absent_and_present_null_separate(self):
        models = importlib.import_module("authored_echo_raw.models")
        for wire in ({}, {"direct_choice": None}, {"alias_choice": None}, {"alias_choice": "raw"}):
            with self.subTest(wire=wire):
                self.assertEqual(models.OptionalMessage.from_dict(wire).to_dict(), wire)
        self.assertEqual(
            models.OptionalMessage(direct_choice=None, ordinary=None).to_dict(),
            {"direct_choice": None},
        )
        self.assertEqual(models.OptionalMessage.from_dict({"ordinary": None}).to_dict(), {})

    def test_transport_request_success_and_lazy_error_use_the_same_codec(self):
        for policy in ("default", "strict", "raw"):
            package = f"authored_echo_{policy}"
            message = importlib.import_module(f"{package}.models.message")
            runtime = importlib.import_module(f"{package}.runtime")
            api = importlib.import_module(f"{package}.apis.default_api")
            wire = {"kind": "text_alt", "label": "message"} if policy != "raw" else None
            for status in (200, 422):
                with self.subTest(policy=policy, status=status):
                    sent_model = message.message_from_dict(wire)
                    http, sent = transport(wire, status)
                    with runtime.Client("https://example.test", http_client=http) as client:
                        endpoint = api.DefaultApi(client)
                        if status == 200:
                            value = endpoint.echo(body=sent_model)
                        else:
                            with self.assertRaises(api.EchoError) as caught:
                                endpoint.echo(body=sent_model)
                            value = caught.exception.detail.body
                            self.assertIs(caught.exception.detail.body, value)
                    if policy != "raw":
                        self.assertIsInstance(value, message.MessageTextCase)
                    self.assertEqual(message.message_to_dict(value), wire)
                    self.assertEqual(sent_json(sent[0]), wire)
