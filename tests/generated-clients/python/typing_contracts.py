"""Checked-in consumer declarations for generated public Python APIs."""

from typing import assert_type

from union_default.apis.default_api import DefaultApi, SendChoiceBadRequest, SendChoiceError
from union_default.models import AcyclicEnvelope, Branch, Choice, ClientTool, NumericTwo, Request, TaggedMemberChoice, TaggedRecord
from union_default.models.choice import choice_from_dict, choice_to_dict
from union_default.models.tagged_member_choice import tagged_member_choice_from_dict, tagged_member_choice_to_dict
from union_raw.apis.default_api import DefaultApi as RawApi
from union_raw.models import Envelope as RawEnvelope
from union_raw.models.choice import choice_from_dict as raw_choice_from_dict
from union_strict.models.choice import choice_from_dict as strict_choice_from_dict
from union_strict.models import Choice as StrictChoice


def typed_models(member: ClientTool, wire: dict[str, object]) -> Request:
    request = Request(tools=[member])
    assert_type(request.to_dict(), dict[str, object])
    assert_type(choice_from_dict(wire), Choice)
    assert_type(strict_choice_from_dict(wire), StrictChoice)
    assert_type(choice_to_dict(NumericTwo(revision=2)), object)
    assert_type(Branch.from_dict(wire).children, list[Branch | str])
    return request


def typed_http(api: DefaultApi, member: NumericTwo, error: SendChoiceError) -> Choice:
    response = api.send_choice_with_http_info(body=member)
    assert_type(response.data, Choice)
    if isinstance(error.detail, SendChoiceBadRequest):
        assert_type(error.detail.body, Choice)
    return api.send_choice(body=member)


def typed_tagged_member(api: DefaultApi, member: TaggedRecord, wire: dict[str, object]) -> TaggedMemberChoice:
    assert_type(AcyclicEnvelope(value=1).value, int | str)
    assert_type(tagged_member_choice_from_dict(wire), TaggedMemberChoice)
    assert_type(tagged_member_choice_to_dict(member), object)
    assert_type(api.send_tagged_member_with_http_info(body=member).data, TaggedMemberChoice)
    return api.send_tagged_member(body=member)


def raw_http(api: RawApi, value: object, envelope: RawEnvelope) -> object:
    assert_type(raw_choice_from_dict(value), object)
    assert_type(envelope.choice, object)
    assert_type(envelope.choice_list, list[object])
    assert_type(envelope.choice_map, dict[str, object])
    return api.send_choice(body=value)
