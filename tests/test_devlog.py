"""Tests for dev-only structured event logging (oxe.devlog)."""

import json
import logging

import pytest

from oxe import devlog


class _Capture(logging.Handler):
    def __init__(self) -> None:
        super().__init__()
        self.records: list[str] = []

    def emit(self, record: logging.LogRecord) -> None:
        self.records.append(record.getMessage())


def test_event_disabled_by_default(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("OXE_DEV", raising=False)
    monkeypatch.setenv("OXE_LOG_LEVEL", "INFO")
    assert devlog.dev_enabled() is False


def test_event_enabled_via_env(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("OXE_DEV", "1")
    assert devlog.dev_enabled() is True
    monkeypatch.delenv("OXE_DEV")
    monkeypatch.setenv("OXE_LOG_LEVEL", "DEBUG")
    assert devlog.dev_enabled() is True


def test_event_emits_single_json_line(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("OXE_DEV", "1")
    cap = _Capture()
    lg = logging.getLogger("oxe.dev")
    lg.addHandler(cap)
    lg.setLevel(logging.DEBUG)
    try:
        devlog.event("search", q="python", page=1, source="cache", results=3)
    finally:
        lg.removeHandler(cap)
    assert len(cap.records) == 1
    fields = json.loads(cap.records[0])
    assert fields["event"] == "search"
    assert fields["q"] == "python"
    assert fields["page"] == 1
    assert fields["source"] == "cache"
    assert fields["results"] == 3
    assert "ts" in fields


def test_event_noop_when_not_dev(
    monkeypatch: pytest.MonkeyPatch, caplog: pytest.LogCaptureFixture
) -> None:
    monkeypatch.delenv("OXE_DEV", raising=False)
    monkeypatch.setenv("OXE_LOG_LEVEL", "INFO")
    with caplog.at_level(logging.DEBUG, logger="oxe.dev"):
        devlog.event("search", q="x")
    assert caplog.records == []
