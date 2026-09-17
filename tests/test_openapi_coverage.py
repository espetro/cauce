"""The contract-coverage gate from the plan's "The API contract is typed in
both directions" section: every operation's 200 response and every mutating
method's request body must resolve to a component ``$ref``, not a bare
``object``/``array``/nothing. SSE routes (``text/event-stream``) are exempt
from the 200-response check since OpenAPI has no JSON schema for a stream
(see ``oxe/api/ai_frames.py``).

Calls ``create_app().openapi()`` live rather than reading the committed
``openapi.json``, so this test cannot pass against a stale file -- drift is
instead caught separately by ``check:openapi`` (``mise.toml``).

``app.openapi()`` is typed upstream as ``dict[str, Any]``; every value
pulled out of it is narrowed via ``_as_dict``/``isinstance`` into
``oxe.jsontypes.JSONDict``/``JSONValue`` before use, the same boundary
pattern ``oxe.config._as_object_dict`` uses for ``tomllib``'s untyped
return, so ``Any`` never leaks past this module's edge.
"""

from typing import cast

from oxe.app import create_app
from oxe.jsontypes import JSONDict, JSONValue

_MUTATING_METHODS = {"post", "put", "patch", "delete"}
_OPERATION_METHODS = {"get", "post", "put", "patch", "delete"}


def _as_dict(value: JSONValue | None) -> JSONDict:
    return cast(JSONDict, value) if isinstance(value, dict) else {}


def _is_component_ref(schema: JSONValue | None) -> bool:
    if not isinstance(schema, dict):
        return False
    ref = schema.get("$ref")
    return isinstance(ref, str) and "/components/schemas/" in ref


def test_every_operation_covers_its_contract() -> None:
    schema = cast(JSONDict, create_app().openapi())
    paths = _as_dict(schema.get("paths"))

    violations: list[str] = []
    for path, raw_methods in paths.items():
        methods = _as_dict(raw_methods)
        for method, raw_operation in methods.items():
            if method not in _OPERATION_METHODS:
                continue
            operation = _as_dict(raw_operation)
            label = f"{method.upper()} {path}"

            responses = _as_dict(operation.get("responses"))
            ok_response = _as_dict(responses.get("200"))
            content = _as_dict(ok_response.get("content"))
            if "text/event-stream" in content:
                continue
            json_content = _as_dict(content.get("application/json"))
            json_schema = json_content.get("schema")
            if not _is_component_ref(json_schema):
                violations.append(f"{label}: 200 response has no component $ref")

            if method in _MUTATING_METHODS:
                request_body = operation.get("requestBody")
                if request_body is None:
                    continue
                body_content = _as_dict(_as_dict(request_body).get("content"))
                body_json = _as_dict(body_content.get("application/json"))
                body_schema = body_json.get("schema")
                if not _is_component_ref(body_schema):
                    violations.append(f"{label}: requestBody has no component $ref")

    assert not violations, "uncovered operations:\n" + "\n".join(violations)
