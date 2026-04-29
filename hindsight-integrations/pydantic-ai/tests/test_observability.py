"""Tests for the optional Logfire instrumentation helper.

These tests don't require ``logfire`` to be installed in the dev environment.
The "logfire installed" path is exercised by patching ``_observability._logfire``
with a stand-in module-like object that records its calls.
"""

from __future__ import annotations

from contextlib import contextmanager
from unittest.mock import MagicMock

import pytest

from hindsight_pydantic_ai import _observability
from hindsight_pydantic_ai._observability import span, truncate


class TestTruncate:
    def test_returns_none_for_none(self) -> None:
        assert truncate(None) is None

    def test_short_text_unchanged(self) -> None:
        assert truncate("hello world") == "hello world"

    def test_long_text_trimmed_with_ellipsis(self) -> None:
        text = "x" * 500
        result = truncate(text)
        assert result is not None
        assert result.endswith("…")
        assert len(result) == 201  # 200 chars + ellipsis

    def test_custom_length(self) -> None:
        result = truncate("hello world", length=5)
        assert result == "hello…"


@pytest.mark.asyncio
async def test_span_yields_none_when_logfire_missing(monkeypatch: pytest.MonkeyPatch) -> None:
    """With logfire absent (the no-op path), span() must yield None."""
    monkeypatch.setattr(_observability, "_logfire", None)
    async with span("hindsight.test", bank_id="b1") as s:
        assert s is None


@pytest.mark.asyncio
async def test_span_uses_logfire_when_present(monkeypatch: pytest.MonkeyPatch) -> None:
    """With a logfire stand-in, span() must call logfire.span() with cleaned attributes."""
    fake_span = MagicMock()
    fake_span.set_attribute = MagicMock()

    @contextmanager
    def _ctx(name, **attrs):
        # Capture the call so the test can assert on it.
        _ctx.captured = {"name": name, "attrs": attrs}
        yield fake_span

    fake_logfire = MagicMock()
    fake_logfire.span = _ctx
    monkeypatch.setattr(_observability, "_logfire", fake_logfire)

    async with span(
        "hindsight.recall",
        bank_id="b1",
        query="user prefs",
        tags=None,  # should be dropped
    ) as s:
        assert s is fake_span
        s.set_attribute("results_count", 3)

    assert _ctx.captured["name"] == "hindsight.recall"
    # None-valued attributes are stripped before being passed to logfire.span()
    assert _ctx.captured["attrs"] == {"bank_id": "b1", "query": "user prefs"}
    fake_span.set_attribute.assert_called_once_with("results_count", 3)


def test_module_imports_without_logfire() -> None:
    """Importing the integration must not crash if logfire is not installed.

    This is the most important guarantee: the package must work for users
    who haven't opted into Logfire. ``_logfire`` is None when ``import logfire``
    raises ImportError at module load time.
    """
    # Just re-import to confirm no exception.
    import importlib

    importlib.reload(_observability)
    # When run in CI without logfire installed, _logfire should be None.
    # When run with logfire installed (dev), it will be the real module.
    # Either way, the import must succeed.
    assert hasattr(_observability, "_logfire")
