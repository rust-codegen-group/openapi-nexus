"""Native consumers of explicit dictionary and mixed-object extensions."""

import importlib
import json
import unittest
from unittest.mock import patch

from test_union import sent_json, transport


class AdditionalPropertiesContracts(unittest.TestCase):
    def models(self, policy="default"):
        return importlib.import_module(f"additional_{policy}.models")

    def test_opaque_extensions_keep_verbatim_keys_and_values(self):
        wire = {
            "known_field": "fixed",
            "knownField": "shadow",
            "extra_key": {"wire_value": "opaque", "nil": None},
            "large": 9007199254740993,
        }
        self.assertEqual(self.models().OpaqueMap.from_dict(wire).to_dict(), wire)

    def test_typed_extensions_keep_declared_models_and_wire_names(self):
        wire = {"known_field": "fixed", "extra_key": {"wire_value": "nested"}}
        value = self.models().MixedMap.from_dict(wire)
        self.assertEqual(value.to_dict(), wire)
        self.assertIsInstance(value.additional_properties["extra_key"], self.models().NestedValue)

    def test_pure_dictionary_exposes_normal_conversion(self):
        wire = {"keep_key": {"wire_value": "nested"}}
        value = self.models().TypedMap.from_dict(wire)
        self.assertEqual(value.to_dict(), wire)
        self.assertIsInstance(value.additional_properties["keep_key"], self.models().NestedValue)

    def test_empty_defaults_are_mutable_and_independent(self):
        for policy in ("default", "strict", "raw"):
            models = self.models(policy)
            first, second = models.TypedMap(), models.TypedMap()
            first.additional_properties["new_key"] = models.NestedValue(wire_value="new")
            self.assertEqual(first.to_dict(), {"new_key": {"wire_value": "new"}})
            self.assertEqual(second.to_dict(), {})
            self.assertEqual(models.MixedMap(known_field="fixed").to_dict(), {"known_field": "fixed"})

    def test_scalar_and_enum_values_keep_native_types_and_errors(self):
        for policy in ("default", "strict", "raw"):
            models = self.models(policy)
            numbers = {"large": 9007199254740993, "negative": -2}
            self.assertEqual(models.Numbers.from_dict(numbers).to_dict(), numbers)
            value = models.StatusValues.from_dict({"status": "active"})
            self.assertIsInstance(value.additional_properties["status"], models.Status)
            self.assertEqual(value.to_dict(), {"status": "active"})
            with self.assertRaises(ValueError):
                models.StatusValues.from_dict({"status": "invalid"})

    def test_from_dict_does_not_mutate_input_and_does_not_hide_model_errors(self):
        models = self.models()
        wire = {"known_field": "fixed", "extra": {"wire_value": "nested"}}
        before = json.loads(json.dumps(wire))
        value = models.MixedMap.from_dict(wire)
        self.assertEqual(wire, before)
        self.assertIsInstance(wire["extra"], dict)
        value.additional_properties.clear()
        self.assertEqual(wire, before)
        with self.assertRaises(KeyError):
            models.NestedValue.from_dict({})
        with self.assertRaises(KeyError):
            models.MixedMap.from_dict({"known_field": "fixed", "extra": {}})
        with self.assertRaises((AttributeError, TypeError)):
            models.TypedMap.from_dict([])

    def test_raw_values_keep_nulls_large_numbers_and_storage_looking_keys(self):
        wire = {"additional_properties": None, "": [True, None], "large": 9007199254740993, "quote'\\key": {"null": None}}
        for policy in ("default", "strict", "raw"):
            value = self.models(policy).RawMap.from_dict(wire)
            self.assertEqual(json.loads(json.dumps(value.to_dict())), wire)
            self.assertEqual(value.additional_properties, wire)

    def test_model_named_any_keeps_opaque_extension_values(self):
        wire = {"nil": None, "nested": {"wire_value": "opaque"}, "large": 9007199254740993}
        for policy in ("default", "strict", "raw"):
            self.assertEqual(self.models(policy).Any.from_dict(wire).to_dict(), wire)

    def test_nullable_and_deep_container_values_keep_declared_types(self):
        for policy in ("default", "strict", "raw"):
            models = self.models(policy)
            nullable = models.NullableValues.from_dict({"nil": None, "model": {"wire_value": "nested"}})
            self.assertIsNone(nullable.additional_properties["nil"])
            self.assertIsInstance(nullable.additional_properties["model"], models.NestedValue)
            self.assertEqual(nullable.to_dict(), {"nil": None, "model": {"wire_value": "nested"}})
            wire = {"list": [{"keep_key": {"wire_value": "nested"}}]}
            value = models.DeepValues.from_dict(wire)
            self.assertIsInstance(value.additional_properties["list"][0]["keep_key"], models.NestedValue)
            self.assertEqual(value.to_dict(), wire)

    def test_alias_and_containing_boundaries_use_the_same_codec(self):
        dictionary = {"keep_key": {"wire_value": "nested"}}
        wire = {"mixed": {"known_field": "fixed", **dictionary}, "opaque": {"known_field": "fixed", "nil": None}, "dictionaries": [dictionary], "aliases": {"alias": dictionary}}
        for policy in ("default", "strict", "raw"):
            models = self.models(policy)
            value = models.Holder.from_dict(wire)
            self.assertIsInstance(value.dictionaries[0], models.TypedMap)
            self.assertIsInstance(value.aliases["alias"], models.TypedMap)
            self.assertEqual(json.loads(json.dumps(value.to_dict())), wire)
            alias_values = models.AliasValues.from_dict(dictionary)
            self.assertIsInstance(alias_values.additional_properties["keep_key"], models.NestedValue)
            self.assertEqual(alias_values.to_dict(), dictionary)

    def test_recursive_dictionary_and_model_dependencies_import_and_convert(self):
        for policy in ("default", "strict", "raw"):
            models = self.models(policy)
            wire = {"wire_value": "root", "child": {"wire_value": "leaf"}}
            value = models.Recursive.from_dict(wire)
            self.assertIsInstance(value.additional_properties["child"], models.Recursive)
            self.assertEqual(value.to_dict(), wire)
            pure = {"one": {"two": {}}}
            self.assertEqual(models.PureRecursive.from_dict(pure).to_dict(), pure)
            nodes = {"root": {"wire_value": "root", "children": {"leaf": {"wire_value": "leaf", "children": {}}}}}
            value = models.Nodes.from_dict(nodes)
            self.assertIsInstance(value.additional_properties["root"].children, models.Nodes)
            self.assertEqual(value.to_dict(), nodes)

    def test_storage_name_does_not_replace_an_authored_field(self):
        wire = {"additionalProperties": "known", "additional_properties": {"wire_value": "extra"}}
        for policy in ("default", "strict", "raw"):
            value = self.models(policy).StorageCollision.from_dict(wire)
            self.assertEqual(value.additional_properties, "known")
            self.assertEqual(value.additional_properties_["additional_properties"].wire_value, "extra")
            self.assertEqual(value.to_dict(), wire)

    def test_storage_factory_is_not_shadowed_by_named_fields(self):
        value = self.models().FactoryCollision.from_dict({"field": "known", "dict": "named", "extra": None})
        self.assertEqual(value.to_dict(), {"field": "known", "dict": "named", "extra": None})

    def test_extra_keys_cannot_overwrite_even_absent_declared_fields(self):
        models = self.models()
        for value in (models.OpaqueMap(known_field="fixed"), models.OptionalKnown()):
            value.additional_properties["known_field"] = "collision"
            with self.assertRaisesRegex(ValueError, "collides with declared wire field"):
                value.to_dict()
        self.assertEqual(models.OptionalKnown.from_dict({"known_field": None, "extra": None}).to_dict(), {"extra": None})

    def test_closed_and_implicit_objects_keep_their_existing_behavior(self):
        wire = {"known_field": "fixed", "extra": None}
        for name in ("Closed", "Implicit"):
            value = getattr(self.models(), name).from_dict(wire)
            self.assertFalse(hasattr(value, "additional_properties"))
            self.assertEqual(value.to_dict(), {"known_field": "fixed"})

    def test_union_valued_extensions_keep_each_policy(self):
        wire = {"model": {"wire_value": "nested"}, "scalar": 2}
        for policy in ("default", "strict", "raw"):
            models = self.models(policy)
            value = models.Choices.from_dict(wire)
            self.assertEqual(json.loads(json.dumps(value.to_dict())), wire)
            if policy != "raw":
                self.assertIsInstance(value.additional_properties["model"], models.NestedValue)
            else:
                self.assertIsInstance(value.additional_properties["model"], dict)
        models = self.models("strict")
        with self.assertRaisesRegex(ValueError, "Ambiguous"):
            models.AmbiguousValues.from_dict({"value": 2})
        with self.assertRaisesRegex(ValueError, "Ambiguous"):
            models.AmbiguousValues(additional_properties={"value": 2}).to_dict()

    def test_tagged_payload_extensions_survive_direct_and_dictionary_conversion(self):
        searching = {"type": "searching", "sequence_number": 1, "sequenceNumber": "shadow", "extra": None}
        completed = {"type": "completed", "extra": {"large": 9007199254740993}}
        wire = {"search": searching, "done": completed}
        for policy in ("default", "strict", "raw"):
            models = self.models(policy)
            module = importlib.import_module(f"additional_{policy}.models.event")
            value = module.event_from_dict(data=searching)
            self.assertIsInstance(value, models.EventSearching)
            self.assertEqual(module.event_to_dict(obj=value), searching)
            value = models.EventValues.from_dict(wire)
            self.assertIsInstance(value.additional_properties["done"], models.EventCompleted)
            self.assertEqual(value.to_dict(), wire)

    def test_containing_optional_and_nullable_fields_keep_the_ordinary_null_policy(self):
        for policy in ("default", "strict", "raw"):
            models = self.models(policy)
            for wire in ({}, {"maybe": None}, {"nullable": None}, {"maybe": None, "nullable": None}):
                ordinary = models.LegacyOptionalHolder.from_dict(wire)
                value = models.OptionalHolder.from_dict(wire)
                self.assertIsNone(value.maybe)
                self.assertIsNone(value.nullable)
                self.assertEqual(value.to_dict(), ordinary.to_dict())
            mixed = {"known_field": "fixed", "extra": {"wire_value": "nested"}}
            wire = {"maybe": mixed, "nullable": mixed}
            value = models.OptionalHolder.from_dict(wire)
            self.assertIsInstance(value.maybe, models.MixedMap)
            self.assertEqual(value.to_dict(), wire)

    def test_dictionary_and_mixed_models_match_outbound_unions_without_serializing_to_probe(self):
        wire = {"keep_key": {"wire_value": "nested"}}
        for policy in ("default", "strict"):
            dictionary = importlib.import_module(f"additional_{policy}.models.dictionary_choice")
            mixed = importlib.import_module(f"additional_{policy}.models.mixed_choice")
            models = self.models(policy)
            value = dictionary.dictionary_choice_from_dict(wire)
            self.assertIsInstance(value, models.TypedMap)
            with patch.object(value, "to_dict", wraps=value.to_dict) as encode:
                self.assertEqual(dictionary.dictionary_choice_to_dict(value), wire)
                encode.assert_called_once_with()
            value = models.MixedMap.from_dict({"known_field": "fixed", **wire})
            with patch.object(value, "to_dict", wraps=value.to_dict) as encode:
                self.assertEqual(mixed.mixed_choice_to_dict(value), {"known_field": "fixed", **wire})
                encode.assert_called_once_with()
        invalid = self.models("strict").MixedMap(known_field="fixed", additional_properties={"bad": 42})
        with self.assertRaisesRegex(ValueError, "No matching"):
            importlib.import_module("additional_strict.models.mixed_choice").mixed_choice_to_dict(invalid)

    def test_http_json_requests_responses_and_typed_errors_keep_extensions(self):
        reply = {"known_field": "fixed", "extra": {"wire_value": "nested"}}
        for policy in ("default", "strict", "raw"):
            module = importlib.import_module(f"additional_{policy}.apis.default_api")
            runtime = importlib.import_module(f"additional_{policy}.runtime")
            models = self.models(policy)
            http, sent = transport(reply)
            with runtime.Client("https://additional.test", http_client=http) as client:
                value = module.DefaultApi(client).echo_mixed(body=models.MixedMap.from_dict(reply))
                self.assertEqual(sent_json(sent[0]), reply)
                self.assertEqual(value.to_dict(), reply)
            dictionary = {"keep_key": {"wire_value": "nested"}}
            http, sent = transport(dictionary)
            with runtime.Client("https://additional.test", http_client=http) as client:
                value = module.DefaultApi(client).echo_dictionary(body=models.TypedMap.from_dict(dictionary))
                self.assertEqual(sent_json(sent[0]), dictionary)
                self.assertEqual(value.to_dict(), dictionary)
            http, sent = transport(dictionary, 400)
            with runtime.Client("https://additional.test", http_client=http) as client:
                with self.assertRaises(module.EchoMixedError) as caught:
                    module.DefaultApi(client).echo_mixed(body=models.MixedMap.from_dict(reply))
                self.assertIsInstance(caught.exception.detail, module.EchoMixedBadRequest)
                self.assertEqual(caught.exception.detail.body.to_dict(), dictionary)
                self.assertIs(caught.exception.detail.body, caught.exception.detail.body)

    def test_typed_http_error_decode_failures_remain_lazy_and_cached(self):
        models = self.models()
        runtime = importlib.import_module("additional_default.runtime")
        module = importlib.import_module("additional_default.apis.default_api")
        http, _ = transport({"bad": {}}, 400)
        with runtime.Client("https://additional.test", http_client=http) as client:
            with patch.object(models.NestedValue, "from_dict", wraps=models.NestedValue.from_dict) as decode:
                with self.assertRaises(module.EchoMixedError) as caught:
                    module.DefaultApi(client).echo_mixed(body=models.MixedMap(known_field="fixed"))
                decode.assert_not_called()
                with self.assertRaises(KeyError) as first:
                    _ = caught.exception.detail.body
                with self.assertRaises(KeyError) as second:
                    _ = caught.exception.detail.body
                self.assertIs(first.exception, second.exception)
                decode.assert_called_once_with({})
