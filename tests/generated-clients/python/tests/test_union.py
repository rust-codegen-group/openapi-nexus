"""Native consumers; generation and environment setup belong to just/CI."""

import importlib
import json
import os
from email import policy as email_policy
from email.parser import BytesParser
import unittest
from unittest.mock import patch


CLIENT = {"name": "weather", "input_schema": {"type": "object"}}
SEARCH = {"type": "web_search_20250305", "name": "web_search", "max_uses": 1}
NUMERIC = {"revision": 2, "second_value": "two"}


def transport(reply, status=200):
    """Exercise the generated transport and JSON encoder without network IO."""
    sent = []
    if os.environ["NEXUS_PYTHON_BACKEND"] == "httpx":
        import httpx

        def handle(request):
            sent.append(request)
            return httpx.Response(status, content=json.dumps(reply), headers={"X-Test": "union", "Content-Type": "application/json"})

        return httpx.Client(transport=httpx.MockTransport(handle)), sent

    import requests
    from requests.adapters import HTTPAdapter

    class Adapter(HTTPAdapter):
        def send(self, request, **kwargs):
            sent.append(request)
            response = requests.Response()
            response.status_code = status
            response._content = json.dumps(reply).encode()
            response.headers["X-Test"] = "union"
            response.request = request
            return response

    session = requests.Session()
    session.mount("https://", Adapter())
    return session, sent


def sent_json(request):
    body = request.content if hasattr(request, "content") else request.body
    return json.loads(body)


class UnionContracts(unittest.TestCase):
    def helper(self, policy, model, direction="from_dict"):
        module = importlib.import_module(f"union_{policy}.models.{model}")
        return getattr(module, f"{model}_{direction}")

    def test_direct_member_controls(self):
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            for name, wire in (("ClientTool", CLIENT), ("SearchTool", SEARCH), ("NumericTwo", NUMERIC)):
                with self.subTest(policy=policy, model=name):
                    self.assertEqual(getattr(models, name).from_dict(wire).to_dict(), wire)

    def test_typed_request_json(self):
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                value = models.Request(tools=[models.ClientTool.from_dict(CLIENT), models.SearchTool.from_dict(SEARCH)])
                self.assertEqual(json.loads(json.dumps(value.to_dict())), {"tools": [CLIENT, SEARCH]})

    def test_inbound_request_members(self):
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                value = models.Request.from_dict({"tools": [CLIENT, SEARCH]})
                self.assertIsInstance(value.tools[0], models.ClientTool)
                self.assertIsInstance(value.tools[1], models.SearchTool)

    def test_nested_scalar_array_map_alias_and_optional(self):
        wire = {"choice": NUMERIC, "choice_list": [NUMERIC], "choice_map": {"keep_key": NUMERIC}, "deep": [{"keep_key": [NUMERIC]}]}
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                value = models.Envelope.from_dict(wire)
                for member in (value.choice, value.choice_list[0], value.choice_map["keep_key"], value.deep[0]["keep_key"][0]):
                    self.assertIsInstance(member, models.NumericTwo)
                self.assertIsNone(value.optional_choice)
                self.assertEqual(json.loads(json.dumps(value.to_dict())), wire)

    def test_disjoint_enums_in_both_directions(self):
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                value = self.helper(policy, "choice")(NUMERIC)
                self.assertIsInstance(value, models.NumericTwo)
                self.assertEqual(self.helper(policy, "choice", "to_dict")(value), NUMERIC)

    def test_first_match_and_strict_ambiguity(self):
        models = importlib.import_module("union_default.models")
        self.assertIsInstance(self.helper("default", "ambiguous")(NUMERIC), models.NumericTwo)
        for direction in ("from_dict", "to_dict"):
            with self.subTest(direction=direction):
                with self.assertRaisesRegex(ValueError, "Ambiguous Ambiguous"):
                    self.helper("strict", "ambiguous", direction)(NUMERIC)

    def test_nested_strict_error_propagates(self):
        with self.assertRaisesRegex(ValueError, "Ambiguous Ambiguous"):
            self.helper("strict", "nested_ambiguous")(NUMERIC)

    def test_unmatched_default_and_raw_preserve_identity(self):
        wire = {"revision": 9, "unknown_key": [1, None, True]}
        for policy in ("default", "raw"):
            for direction in ("from_dict", "to_dict"):
                with self.subTest(policy=policy, direction=direction):
                    self.assertIs(self.helper(policy, "choice", direction)(wire), wire)
        for direction in ("from_dict", "to_dict"):
            with self.assertRaisesRegex(ValueError, "No matching Choice"):
                self.helper("strict", "choice", direction)(wire)

    def test_open_fallback_does_not_shadow_concrete(self):
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                self.assertIsInstance(self.helper(policy, "open_choice")(NUMERIC), models.NumericTwo)
                wire = {"other_key": True}
                self.assertIs(self.helper(policy, "open_choice")(wire), wire)

    def test_numeric_matching_is_not_python_boolean_equality(self):
        for policy in ("default", "strict"):
            for value in (1, 2, True):
                self.assertIs(self.helper(policy, "scalar_choice")(value), value)
        for value in (False, 3, 1.5):
            with self.subTest(value=value):
                with self.assertRaises(ValueError):
                    self.helper("strict", "scalar_choice")(value)

    def test_nullable_union(self):
        for policy in ("default", "strict", "raw"):
            for direction in ("from_dict", "to_dict"):
                self.assertIsNone(self.helper(policy, "nullable_choice", direction)(None))

    def test_authored_nulls_keep_strict_counts_and_nested_errors(self):
        for model in ("inner_null_choice", "wrapped_null_choice"):
            for direction in ("from_dict", "to_dict"):
                with self.subTest(model=model, direction=direction):
                    with self.assertRaisesRegex(ValueError, "Ambiguous InnerNullChoice"):
                        self.helper("strict", model, direction)(None)
        models = importlib.import_module("union_strict.models")
        with self.assertRaisesRegex(ValueError, "Ambiguous InnerNullChoice"):
            models.NullableEnvelope.from_dict({"choice": None, "ambiguous": None})

    def test_required_nullable_field_preserves_explicit_null(self):
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                self.assertEqual(models.NullableEnvelope.from_dict({"choice": None}).to_dict(), {"choice": None})

    def test_nullable_finite_wrapper_restores_its_conversion_boundary(self):
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            for value in (None, 1, True):
                with self.subTest(policy=policy, value=value):
                    wire = {"value": value}
                    self.assertEqual(models.FiniteEnvelope.from_dict(wire).to_dict(), wire)

    def test_recursive_member_conversion_and_imports(self):
        wire = {"children": [{"children": ["leaf"]}]}
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                value = self.helper(policy, "recursive")(wire)
                self.assertIsInstance(value, models.Branch)
                self.assertIsInstance(value.children[0], models.Branch)
                self.assertEqual(self.helper(policy, "recursive", "to_dict")(value), wire)

    def test_acyclic_model_dependencies_do_not_gain_helper_import_cycles(self):
        wire = {"envelope": {"value": 1}}
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                envelope = models.AcyclicEnvelope(value=1)
                self.assertEqual(envelope.to_dict(), {"value": 1})
                dependent = models.DependentEnvelope.from_dict(wire)
                self.assertIsInstance(dependent.envelope, models.AcyclicEnvelope)
                self.assertEqual(dependent.to_dict(), wire)
                if policy != "raw":
                    value = self.helper(policy, "dependent_choice")(wire)
                    self.assertIsInstance(value, models.DependentEnvelope)
                    self.assertEqual(self.helper(policy, "dependent_choice", "to_dict")(value), wire)

    def test_tagged_members_keep_their_public_payload_representation(self):
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            for tag, payload, model in (("record", {"name": "weather"}, models.TaggedRecord), ("numeric", NUMERIC, models.NumericTwo)):
                wire = {"kind": tag, "payload": payload}
                with self.subTest(policy=policy, tag=tag):
                    value = self.helper(policy, "tagged_member_choice")(wire)
                    self.assertIsInstance(value, model)
                    self.assertEqual(self.helper(policy, "adjacent_member", "to_dict")(value), wire)
                    self.assertEqual(self.helper(policy, "tagged_member_choice", "to_dict")(value), wire)

    def test_tagged_members_match_through_aliases_containers_and_objects(self):
        member = {"kind": "record", "payload": {"name": "weather"}}
        wire = {"value": member, "values": [member], "mapping": {"keep_key": member}}
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                value = self.helper(policy, "tagged_object_choice")(wire)
                self.assertIsInstance(value, models.TaggedMemberEnvelope)
                for item in (value.value, value.values[0], value.mapping["keep_key"]):
                    self.assertIsInstance(item, models.TaggedRecord)
                self.assertEqual(json.loads(json.dumps(self.helper(policy, "tagged_object_choice", "to_dict")(value))), wire)
                values = self.helper(policy, "tagged_container_choice")([member])
                self.assertIsInstance(values[0], models.TaggedRecord)
                self.assertEqual(self.helper(policy, "tagged_container_choice", "to_dict")(values), [member])

    def test_tagged_member_selection_counts_before_calling_the_delegate(self):
        models = importlib.import_module("union_strict.models")
        tagged = importlib.import_module("union_strict.models.adjacent_member")
        member = models.TaggedRecord(name="weather")
        with patch.object(tagged, "adjacent_member_to_dict", wraps=tagged.adjacent_member_to_dict) as encode:
            with self.assertRaisesRegex(ValueError, "Ambiguous AmbiguousTaggedMember"):
                self.helper("strict", "ambiguous_tagged_member", "to_dict")(member)
            encode.assert_not_called()
            wire = self.helper("strict", "tagged_member_choice", "to_dict")(member)
            self.assertEqual(wire, {"kind": "record", "payload": {"name": "weather"}})
            encode.assert_called_once_with(member)

    def test_raw_tagged_member_subtrees_preserve_wire_identity(self):
        models = importlib.import_module("union_raw.models")
        member = {"kind": "record", "payload": {"name": "weather"}}
        for direction in ("from_dict", "to_dict"):
            self.assertIs(self.helper("raw", "tagged_member_choice", direction)(member), member)
        wire = {"value": member, "values": [member], "mapping": {"keep_key": member}}
        value = models.TaggedMemberEnvelope.from_dict(wire)
        self.assertIs(value.value, member)
        self.assertEqual(value.to_dict(), wire)

    def test_raw_union_subtree_does_not_disable_parent_conversion(self):
        models = importlib.import_module("union_raw.models")
        wire = {"choice": NUMERIC, "choice_list": [NUMERIC], "choice_map": {"keep_key": NUMERIC}, "deep": [{"keep_key": [NUMERIC]}]}
        value = models.Envelope.from_dict(wire)
        self.assertIsInstance(value, models.Envelope)
        self.assertIs(value.choice, NUMERIC)
        self.assertEqual(value.to_dict(), wire)

    def test_select_before_delegate_and_convert_once(self):
        models = importlib.import_module("union_strict.models")
        with patch.object(models.NumericTwo, "from_dict", wraps=models.NumericTwo.from_dict) as decode:
            with self.assertRaisesRegex(ValueError, "Ambiguous"):
                self.helper("strict", "ambiguous")(NUMERIC)
            decode.assert_not_called()
            self.helper("strict", "choice")(NUMERIC)
            decode.assert_called_once_with(NUMERIC)

    def test_http_request_success_and_metadata(self):
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            runtime = importlib.import_module(f"union_{policy}.runtime")
            api = importlib.import_module(f"union_{policy}.apis.default_api")
            http, sent = transport(NUMERIC)
            with self.subTest(policy=policy), runtime.Client("https://example.test", http_client=http) as client:
                member = NUMERIC if policy == "raw" else models.NumericTwo.from_dict(NUMERIC)
                response = api.DefaultApi(client).send_choice_with_http_info(body=member)
                self.assertEqual(sent_json(sent[0]), NUMERIC)
                self.assertEqual(response.status_code, 200)
                self.assertEqual(response.headers["X-Test"], "union")
                if policy == "raw":
                    self.assertEqual(response.data, NUMERIC)
                else:
                    self.assertIsInstance(response.data, models.NumericTwo)
                    self.assertEqual(response.data.to_dict(), NUMERIC)

    def test_http_array_request_map_response(self):
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            runtime = importlib.import_module(f"union_{policy}.runtime")
            api = importlib.import_module(f"union_{policy}.apis.default_api")
            http, sent = transport({"keep_key": NUMERIC})
            with self.subTest(policy=policy), runtime.Client("https://example.test", http_client=http) as client:
                value = api.DefaultApi(client).send_choices(body=[models.NumericTwo.from_dict(NUMERIC)])
                self.assertEqual(sent_json(sent[0]), [NUMERIC])
                self.assertIsInstance(value["keep_key"], models.NumericTwo)

    def test_http_error_body_is_typed_lazy_and_cached(self):
        for policy in ("default", "strict"):
            models = importlib.import_module(f"union_{policy}.models")
            runtime = importlib.import_module(f"union_{policy}.runtime")
            api = importlib.import_module(f"union_{policy}.apis.default_api")
            http, _ = transport(NUMERIC, status=400)
            with self.subTest(policy=policy), runtime.Client("https://example.test", http_client=http) as client:
                with patch.object(models.NumericTwo, "from_dict", wraps=models.NumericTwo.from_dict) as decode:
                    member = models.NumericTwo(revision=2, second_value="two")
                    with self.assertRaises(api.SendChoiceError) as caught:
                        api.DefaultApi(client).send_choice(body=member)
                    decode.assert_not_called()
                    value = caught.exception.detail.body
                    self.assertIsInstance(value, models.NumericTwo)
                    self.assertIs(caught.exception.detail.body, value)
                    decode.assert_called_once_with(NUMERIC)

    def test_http_required_null_is_json_null_not_omitted(self):
        for policy in ("default", "strict", "raw"):
            runtime = importlib.import_module(f"union_{policy}.runtime")
            api = importlib.import_module(f"union_{policy}.apis.default_api")
            http, sent = transport(None)
            with self.subTest(policy=policy), runtime.Client("https://example.test", http_client=http) as client:
                self.assertIsNone(api.DefaultApi(client).send_nullable(body=None))
                self.assertIsNone(sent_json(sent[0]))

    def test_multipart_json_part_uses_the_same_member_codec(self):
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            runtime = importlib.import_module(f"union_{policy}.runtime")
            api = importlib.import_module(f"union_{policy}.apis.default_api")
            http, sent = transport(None, status=204)
            with self.subTest(policy=policy), runtime.Client("https://example.test", http_client=http) as client:
                member = NUMERIC if policy == "raw" else models.NumericTwo.from_dict(NUMERIC)
                body = models.SendMultipartMultipartRequestBody(choice=member)
                api.DefaultApi(client).send_multipart(body=body)
                request = sent[0]
                content = request.content if hasattr(request, "content") else request.body
                message = BytesParser(policy=email_policy.default).parsebytes(
                    ("Content-Type: " + request.headers["Content-Type"] + "\r\nMIME-Version: 1.0\r\n\r\n").encode() + content
                )
                parts = {part.get_param("name", header="content-disposition"): part for part in message.iter_parts()}
                self.assertEqual(json.loads(parts["choice"].get_payload(decode=True)), NUMERIC)

    def test_tagged_payload_keeps_untagged_conversion_and_policy(self):
        wire = {"kind": "choice", "payload": NUMERIC}
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            with self.subTest(policy=policy):
                value = self.helper(policy, "tagged_choice")(wire)
                if policy == "raw":
                    self.assertIs(value, NUMERIC)
                else:
                    self.assertIsInstance(value, models.NumericTwo)
                self.assertEqual(self.helper(policy, "tagged_choice", "to_dict")(value), wire)
        with self.assertRaisesRegex(ValueError, "Ambiguous Ambiguous"):
            self.helper("strict", "tagged_ambiguous")(wire)

    def test_http_tagged_payload_uses_the_shared_union_boundary(self):
        wire = {"kind": "choice", "payload": NUMERIC}
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            runtime = importlib.import_module(f"union_{policy}.runtime")
            api = importlib.import_module(f"union_{policy}.apis.default_api")
            http, sent = transport(wire)
            with self.subTest(policy=policy), runtime.Client("https://example.test", http_client=http) as client:
                member = NUMERIC if policy == "raw" else models.NumericTwo.from_dict(NUMERIC)
                value = api.DefaultApi(client).send_tagged(body=member)
                self.assertEqual(sent_json(sent[0]), wire)
                if policy == "raw":
                    self.assertEqual(value, NUMERIC)
                else:
                    self.assertIsInstance(value, models.NumericTwo)

    def test_http_tagged_member_inside_untagged_union(self):
        wire = {"kind": "record", "payload": {"name": "weather"}}
        for policy in ("default", "strict", "raw"):
            models = importlib.import_module(f"union_{policy}.models")
            runtime = importlib.import_module(f"union_{policy}.runtime")
            api = importlib.import_module(f"union_{policy}.apis.default_api")
            http, sent = transport(wire)
            with self.subTest(policy=policy), runtime.Client("https://example.test", http_client=http) as client:
                member = wire if policy == "raw" else models.TaggedRecord(name="weather")
                response = api.DefaultApi(client).send_tagged_member_with_http_info(body=member)
                self.assertEqual(sent_json(sent[0]), wire)
                if policy == "raw":
                    self.assertEqual(response.data, wire)
                else:
                    self.assertIsInstance(response.data, models.TaggedRecord)
                    self.assertEqual(response.data.to_dict(), {"name": "weather"})


if __name__ == "__main__":
    unittest.main()
