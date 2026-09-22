# Copyright (c) 2026 Joaquin Terrasa and cauce contributors. Apache-2.0.
"""Protocol-level unit tests for cauce_engine_sdk.run (stdlib unittest only).

Run: `python3 sdk/python/tests/test_sdk.py` or
`python3 -m unittest` from this directory.
"""
import io
import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cauce_engine_sdk import Request, Response, Result, run  # noqa: E402


def run_loop(stdin_text: str, fn) -> list[dict]:
    out = io.StringIO()
    with patch("sys.stdin", io.StringIO(stdin_text)), patch("sys.stdout", out):
        run(fn)
    return [json.loads(line) for line in out.getvalue().splitlines()]


class RunLoopTests(unittest.TestCase):
    def test_malformed_line_returns_error_not_crash(self):
        resps = run_loop("this is not json\n", lambda req: [])
        self.assertEqual(len(resps), 1)
        self.assertEqual(resps[0]["v"], 1)
        self.assertEqual(resps[0]["results"], [])
        self.assertIsInstance(resps[0]["error"], str)

    def test_wrong_version_returns_error(self):
        resps = run_loop('{"v":2,"query":"x"}\n', lambda req: [])
        self.assertIsNotNone(resps[0]["error"])

    def test_missing_query_returns_error(self):
        resps = run_loop('{"v":1,"page":1}\n', lambda req: [])
        self.assertIsNotNone(resps[0]["error"])

    def test_valid_request_round_trip(self):
        def echo(req: Request):
            return [Result(title=req.query, url="https://example.com/", snippet="s")]

        resps = run_loop(
            '{"v":1,"query":"hello","page":2,"lang":"en","timeout_ms":1000}\n', echo
        )
        self.assertEqual(resps[0]["error"], None)
        self.assertEqual(resps[0]["results"][0]["title"], "hello")
        self.assertEqual(resps[0]["results"][0]["url"], "https://example.com/")

    def test_handler_exception_becomes_error_response(self):
        def boom(req):
            raise RuntimeError("upstream blew up")

        resps = run_loop('{"v":1,"query":"x"}\n', boom)
        self.assertEqual(resps[0]["results"], [])
        self.assertTrue(resps[0]["error"].startswith("transport:"))

    def test_response_passthrough_keeps_error(self):
        def fail(req):
            return Response(error="rate_limited")

        resps = run_loop('{"v":1,"query":"x"}\n', fail)
        self.assertEqual(resps[0]["error"], "rate_limited")

    def test_eof_exits_cleanly(self):
        self.assertEqual(run_loop("", lambda req: []), [])

    def test_blank_lines_skipped(self):
        resps = run_loop('\n{"v":1,"query":"x"}\n\n', lambda req: [])
        self.assertEqual(len(resps), 1)
        self.assertIsNone(resps[0]["error"])


class DdgsAutoTests(unittest.TestCase):
    """ddgs_auto emits `no_results` instead of `Ok([])` on an empty page
    (issue #120); `ddgs` is stubbed so the test needs no network/extra."""

    def _search_with_hits(self, hits):
        import types

        from cauce_engine_sdk import ddgs_auto

        fake_ddgs = types.SimpleNamespace(
            DDGS=lambda: types.SimpleNamespace(text=lambda *a, **k: hits)
        )
        with patch.dict(sys.modules, {"ddgs": fake_ddgs}):
            return ddgs_auto.search(Request(query="x"))

    def test_empty_hits_emit_no_results(self):
        resp = self._search_with_hits([])
        self.assertEqual(resp.error, "no_results")
        self.assertEqual(resp.results, [])

    def test_hits_without_href_emit_no_results(self):
        resp = self._search_with_hits([{"title": "t", "body": "s"}])
        self.assertEqual(resp.error, "no_results")
        self.assertEqual(resp.results, [])

    def test_hits_return_results_without_error(self):
        resp = self._search_with_hits(
            [{"title": "t", "href": "https://example.com/", "body": "s"}]
        )
        self.assertIsNone(resp.error)
        self.assertEqual(resp.results[0].url, "https://example.com/")


if __name__ == "__main__":
    unittest.main()
