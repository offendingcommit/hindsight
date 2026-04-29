"""Optional Pydantic Logfire instrumentation for hindsight-pydantic-ai.

If ``logfire`` is installed in the environment and the user has called
``logfire.configure()`` somewhere in their app, every retain/recall/reflect
tool call emits a semantically named span with bank id, query, and result
size attributes. If ``logfire`` is not installed, this module imports
cleanly and ``span()`` returns a ``nullcontext()`` so there is zero overhead
and zero behavior change.

Privacy: spans intentionally never carry the raw memory content. The
``query`` attribute is truncated; ``content`` is reduced to a length count.

Usage (inside the integration, not user code)::

    from ._observability import span, truncate

    async with span(
        "hindsight.recall",
        bank_id=bank_id,
        query=truncate(query),
        budget=effective_budget,
    ) as s:
        response = await client.arecall(...)
        if s is not None:
            s.set_attribute("results_count", len(response.results))
"""

from __future__ import annotations

from contextlib import asynccontextmanager
from typing import Any, AsyncIterator

try:  # pragma: no cover — exercised by test_observability
    import logfire as _logfire
except ImportError:  # pragma: no cover
    _logfire = None  # type: ignore[assignment]


_QUERY_TRUNCATE = 200


def truncate(text: str | None, length: int = _QUERY_TRUNCATE) -> str | None:
    """Trim a free-text attribute to a safe length for span attributes."""
    if text is None:
        return None
    if len(text) <= length:
        return text
    return text[:length] + "…"


@asynccontextmanager
async def span(name: str, **attributes: Any) -> AsyncIterator[Any]:
    """Yield a Logfire span if available, else None.

    The yielded value is the underlying logfire span object when logfire is
    installed and configured, otherwise ``None``. Callers can do
    ``if s is not None: s.set_attribute(...)`` to add post-call attributes
    (e.g. result counts) without branching for the no-op case.
    """
    if _logfire is None:
        yield None
        return

    # Drop None-valued attributes — keeps the dashboard clean.
    cleaned = {k: v for k, v in attributes.items() if v is not None}
    with _logfire.span(name, **cleaned) as s:
        yield s
