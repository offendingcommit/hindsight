---
title: "How Hindsight Scales"
description: "A deep dive into how Hindsight's memory operations scale with data volume — performance, quality, and cost across retain, recall, consolidation, and reflect."
authors: [nicoloboschi]
date: 2026-05-08T12:00
tags: [scaling, performance, architecture, engineering]
image: /img/blog/how-hindsight-scales.png
hide_table_of_contents: true
---

Agent memory systems face a scaling problem that traditional databases don't. It's not just "can we store more data" — it's "does the system stay fast, accurate, and affordable as memories pile up over weeks, months, and years."

The challenge is that agent memory involves LLM calls, semantic search, graph traversal, and synthesis. Each has its own scaling curve. Some scale with input size. Some scale with the total number of stored memories. Some scale with query complexity. If you don't understand which is which, you can't predict costs, and you can't tune the system.

This post walks through how each of Hindsight's four core operations — **retain**, **recall**, **consolidation**, and **reflect** — scales across three dimensions: performance, quality, and cost.

<!-- truncate -->

## Retain — Ingesting Memories at Scale

Retain is the write path. Content comes in, gets chunked, facts are extracted by an LLM, embeddings are generated, and everything gets stored with entity, temporal, semantic, and causal links.

### Performance

The retain pipeline is a streaming producer-consumer system. Content is split into chunks of ~3,000 characters, grouped into mini-batches of 100, and processed through three phases:

1. **Phase 1 (read-heavy, outside transaction):** Entity resolution via trigram GIN scan, semantic ANN search to find similar existing facts. Runs on a separate connection to avoid holding row locks during slow reads.
2. **Phase 2 (write transaction):** Insert facts, create entity links, build temporal links (within 24-hour windows), semantic links (within-batch + pre-computed ANN), and causal links. Atomic per batch.
3. **Phase 3 (post-transaction, best-effort):** Final ANN pass across the full bank — finds semantic neighbors for newly inserted facts against the entire existing corpus.

LLM fact extraction — the slowest step — runs up to 32 chunks concurrently. This means retain latency scales as `max(chunk_count / 32, single_chunk_latency)`, not as the sum of all chunks.

The critical scaling distinction: **retain performance scales with input size, not with bank size.** The number of LLM calls, embeddings, and DB writes are all proportional to how much content you're ingesting. The exception is Phase 3's ANN pass, which queries the full bank — but HNSW gives us O(log N) per query, so even at millions of facts this stays fast.

**Delta retain** makes repeated ingestion cheap. If a document's content hash matches a previous version, unchanged chunks are skipped entirely. Only new or modified chunks trigger LLM extraction. For an integration that periodically re-syncs a document, this means the second sync costs nearly nothing.

### Quality

Fact extraction quality is independent of bank size. Each chunk is processed in isolation — the LLM sees only its ~3,000 characters and extracts structured facts from them. Whether the bank has 100 or 100,000 existing facts doesn't change extraction quality.

What does improve with scale is **link density**. More facts in the bank means more temporal neighbors within 24-hour windows, more semantic neighbors above the similarity threshold, and richer entity co-occurrence graphs. This is a quality flywheel: the more memories you store, the more connected the graph becomes, and the better graph-based retrieval works downstream.

Entity resolution also improves with scale. More co-occurrence data means better disambiguation — "John" in a bank with extensive context about "John Smith" and "John Doe" resolves more accurately than in a near-empty bank.

### Cost

| Cost factor | Scales with | Typical magnitude |
|---|---|---|
| LLM extraction | Input chunk count | 1 call per ~3,000 chars (~80% of retain time) |
| Embeddings | Extracted fact count | 1 embedding per fact (free with local model) |
| DB writes | Extracted fact count | Linear — facts, entities, links |
| ANN link creation | Bank size × new fact count | O(N log N) for Phase 3 final pass |

The key insight: **retain cost is proportional to what you're ingesting, not to what's already stored.** A retain call that processes 10 chunks costs the same whether the bank has 100 or 1,000,000 existing facts. The LLM is the dominant cost (80% of wall-clock time), and LLM calls are a function of input volume.

## Recall — Retrieval That Doesn't Degrade

Recall is the read path. It runs four retrieval strategies in parallel — semantic, BM25, graph, and temporal — fuses them with Reciprocal Rank Fusion, and reranks with a cross-encoder. We covered the [full architecture in a previous post](/blog/2026/03/27/parallel-hybrid-search). Here we focus on how it scales.

### Performance

The headline number: **recall makes zero LLM calls.** It's purely retrieval plus a local cross-encoder reranker. This is a deliberate architectural choice — we pay the LLM cost at retain time (fact extraction) so that recall can be fast and free.

Each of the four retrieval strategies has its own scaling profile:

- **Semantic search:** HNSW index gives O(log N) query time. We use per-bank, per-fact-type partial indexes, so the planner hits exactly the right index for each query arm. At 1M facts, a semantic query still completes in single-digit milliseconds.
- **BM25:** PostgreSQL GIN indexes scale well with corpus size. Full-text search latency grows sub-linearly — doubling the corpus doesn't double query time.
- **Graph traversal:** Bounded by a configurable `thinking_budget` (LOW=100, MID=300, HIGH=600 nodes). The budget caps traversal regardless of how dense the link graph gets, so graph retrieval time is effectively constant.
- **Temporal search:** Bounded spreading with a maximum of 5 iterations and 10 neighbors per source unit. BRIN indexes on temporal columns keep range queries fast.

The four strategies run in parallel (semantic + BM25 + temporal share a connection; graph runs independently per fact type). Total recall latency is the max of the slowest branch, not the sum. In practice: **100–600ms regardless of bank size**, with the cross-encoder reranker and connection pool acquisition as the typical bottlenecks, not query speed.

### Quality

This is the most important scaling dimension for recall. As banks grow from hundreds to hundreds of thousands of facts, does retrieval precision degrade?

**Semantic search** uses HNSW, which is approximate. At very large scale, HNSW can miss relevant results. We mitigate this two ways: over-fetching by 5x (request 100 candidates to return 20) and setting `ef_search=200` globally for better recall on sparse graphs. The approximation error is small and bounded — HNSW doesn't suddenly collapse at scale, it just gets slightly less precise.

**BM25** is lexical and doesn't degrade with volume. If the query tokens match the document tokens, BM25 finds it. More documents means more noise in the result set, but RRF fusion and reranking filter that out.

**Graph retrieval actually improves with more data.** A richer link graph means more traversal paths between semantically related facts. Entity co-occurrence, semantic kNN links, and causal chains all become denser over time. This is the opposite of degradation — graph retrieval gets better as the bank grows.

**Temporal retrieval** is bounded by design (5 iterations, 10 neighbors/source), so it doesn't degrade. It also doesn't improve — it's stable regardless of bank size.

The ensemble effect matters here. Even if one strategy gets slightly noisier at scale, the other three compensate. RRF fusion is rank-based (no score normalization needed), so it handles mixed-quality inputs naturally. The cross-encoder reranker then makes the final relevance judgment — and it operates on the merged candidate set, not on any single strategy's output.

The net result: **recall quality is stable or improving as banks grow**, with semantic search as the only strategy that degrades slightly (and even that is well-mitigated).

### Cost

| Cost factor | Scales with | Typical magnitude |
|---|---|---|
| LLM calls | Nothing — always 0 | $0 per recall |
| Cross-encoder reranking | Candidate count (20–100) | ~1–5ms per pair on GPU, local model |
| DB queries | Bank size (O(log N) for semantic) | 3–12 queries per recall |
| Connection pool | Concurrent recall requests | 1 shared + N graph connections |

Recall is "free" in terms of API costs. The cross-encoder is a local 6M-parameter model that runs on CPU. The only cost is compute time and database queries. This is the payoff of the read-write asymmetry: we invested at retain time so recall can be cheap at any scale.

## Consolidation — Background Knowledge Synthesis

Consolidation runs after retain completes. It takes raw experience and world facts and synthesizes them into **observations** — consolidated knowledge that represents higher-level patterns and insights. Think of it as the system "thinking about" what it learned.

### Performance

Consolidation runs asynchronously as a background worker. It never blocks user-facing operations. The pipeline:

1. Fetch unconsolidated memories (batch of 50)
2. For each memory, run a recall to find related existing observations (N parallel recalls)
3. Group memories into sub-batches of 8 and make one LLM call per sub-batch
4. Execute the LLM's instructions: create new observations, update existing ones, or delete stale ones
5. Generate embeddings for new/updated observations
6. Checkpoint: mark memories as consolidated

Throughput is ~0.7–1.0 operations per second, with the LLM accounting for 80–87% of wall-clock time. This is from real consolidation benchmark data — the breakdown is consistently LLM-dominated regardless of bank size or observation density.

The batch architecture has built-in backpressure: `consolidation_max_memories_per_round` (default 100) caps how much work a single round does. If more memories are waiting, the round completes and the next one picks up where it left off. This prevents a large retain from monopolizing the worker pool.

Adaptive error handling keeps things moving: if an LLM call fails for a sub-batch of 8, the system bisects (8→4→2→1) and retries. One bad memory doesn't block the other 99.

### Quality

Consolidation quality **improves with scale**. More raw facts mean richer source material for observation synthesis. An observation about "user prefers functional programming" becomes more confident and nuanced when it's synthesized from 50 relevant facts rather than 3.

Scope isolation (tag-based) prevents cross-context contamination. Memories tagged for different contexts never get consolidated together — a strict security boundary that also helps quality by keeping observations focused.

`max_observations_per_scope` prevents runaway growth. Without it, a bank with 100,000 facts could generate thousands of observations, most of them redundant. The cap forces the LLM to update existing observations rather than creating duplicates.

Source fact tracking preserves provenance. Every observation records which raw facts it was synthesized from. This means you can always trace an observation back to its source material — useful for debugging and for reflect (which can show the user why it believes something).

### Cost

| Cost factor | Scales with | Typical magnitude |
|---|---|---|
| LLM calls | New memory count ÷ 8 | 100 memories → ~13 LLM calls |
| Recalls (finding related observations) | New memory count | 1 recall per memory (DB-only, no LLM) |
| Embeddings | Observations created/updated | 1 per observation (free with local model) |
| DB writes | Observation mutations | Linear with create/update/delete count |

Consolidation is the most LLM-intensive operation per unit of work. But two factors keep costs manageable:

1. **It runs asynchronously.** You can use batch APIs (50% cost reduction) since latency doesn't matter for background work.
2. **It's sub-linear in the long run.** Early on, most memories create new observations. As the bank matures, more memories update existing observations rather than creating new ones. The LLM call count stays at N/8, but the observation count grows slower than the memory count.

## Reflect — Agentic Reasoning at Scale

Reflect is the synthesis operation. Given a question, it searches through a three-tier knowledge hierarchy — mental models, then observations, then raw facts — using an agentic LLM loop that decides what to search for and when it has enough context to answer.

### Performance

The reflect agent follows a forced search sequence before entering free-form reasoning:

1. **Search mental models** (forced, no LLM): Vector search on pre-computed mental model embeddings. Returns cached synthesis results with staleness metadata.
2. **Search observations** (forced, no LLM): Calls recall internally, filtered to observation-type facts. Returns consolidated knowledge.
3. **Search raw facts** (forced, no LLM): Calls recall for experience and world facts. Returns the source material.
4. **Reasoning iterations** (1–7 LLM calls): The agent decides whether to expand results, run additional searches, or synthesize a final answer.
5. **Final synthesis** (1 LLM call): Produces the answer.

Typical reflect latency: 800ms–3s, dominated by LLM generation time. The three forced searches add 100–600ms total (they're just recall operations). The context accumulation is capped at 100K tokens — if the agent accumulates more than that, it's forced into final synthesis.

A wall-clock timeout of 300 seconds (configurable) prevents runaway reflect operations.

### Quality

The three-tier hierarchy is the key quality-at-scale mechanism:

**Mental models** are the top tier. They're user-defined questions with pre-computed, periodically refreshed answers. A well-maintained mental model answers a common question instantly from cache, with no degradation as the bank grows. The freshness is maintained by consolidation triggers — when new observations are created that match a mental model's tags, the model gets refreshed asynchronously.

**Observations** are the middle tier. They represent consolidated knowledge — more stable and concise than raw facts, less curated than mental models. As the bank grows, observations absorb complexity. Instead of reflect needing to reason over 10,000 raw facts, it reasons over 200 observations that summarize them.

**Raw facts** are the fallback. If mental models and observations don't cover the question, the agent searches raw facts. This is where bank size matters most — but by the time the agent reaches this tier, it already has context from the higher tiers to focus its search.

The hierarchical approach means **reflect quality is decoupled from total memory count**. What matters is the quality of observations and mental models, which improve with consolidation over time. The raw fact search is a targeted fallback, not a full-corpus scan.

Mental model **delta refresh** is an additional quality mechanism. Instead of regenerating a mental model from scratch, delta mode identifies which sections are stale and updates only those. This preserves manually curated sections while keeping data-driven sections fresh. It costs one extra LLM call but produces more consistent results than full regeneration.

### Cost

| Cost factor | Scales with | Typical magnitude |
|---|---|---|
| LLM calls (reasoning) | Query complexity | 2–7 per reflect |
| Internal recalls | Fixed at 1–3 | DB-only, no LLM cost |
| Mental model refresh | Triggered by consolidation | 5–8 LLM calls each (async, background) |
| Delta ops (if enabled) | +1 LLM call per refresh | Only for delta-mode mental models |

The cost multiplication chain matters: N new memories → consolidation creates/updates M observations → K mental models with matching tags get refreshed → K × 5 LLM calls. For a bank with 10 mental models and frequent retains, this background cost adds up. But it's all asynchronous — the user-facing reflect operation is just 2–7 LLM calls.

Mental models amortize reflect cost. A question that's answered by a mental model costs one vector search (milliseconds, no LLM). The same question without a mental model costs 2–7 LLM calls. If the question gets asked frequently, the mental model pays for itself quickly.

## The Big Picture

Here's how the four operations compare:

| Operation | LLM Calls | Primary Scaling Dimension | Typical Latency | Cost Driver |
|---|---|---|---|---|
| **Retain** | 1 per chunk | Input size (linear) | 500ms–2s per batch | LLM extraction |
| **Recall** | 0 | Bank size (O(log N)) | 100–600ms | DB + CPU only |
| **Consolidation** | N ÷ 8 | New memory count (linear) | Background | LLM synthesis |
| **Reflect** | 2–7 | Query complexity | 800ms–3s | LLM reasoning |

Five architectural decisions make this work:

**Read-write asymmetry.** We pay the LLM cost at write time (retain extracts structured facts) so that read time (recall) is LLM-free. This is the single biggest scaling lever — recall is the hot path in any agent memory system, and making it free means usage scales without API cost scaling.

**Hierarchical knowledge compression.** Raw facts → observations → mental models. Each tier compresses the one below it. As the bank grows, the higher tiers absorb complexity so that downstream operations (reflect, recall) don't need to touch the full corpus. This is biomimetic — it mirrors how human memory consolidates detailed experiences into general knowledge over time.

**Parallel everything.** Four-way recall, 32-way extraction, async consolidation. Parallelism converts scaling problems from "everything slows down" to "the slowest branch sets the pace." Connection sharing and bounded budgets keep parallelism from creating resource contention.

**Bounded traversal.** Thinking budgets, link caps (20 temporal links per unit), iteration limits (5 for temporal spreading, 10 for reflect), and token ceilings (100K context for reflect). Every operation has a worst-case bound. Nothing can run away, regardless of bank size or query complexity.

**Local models where possible.** Embeddings (sentence-transformers) and reranking (cross-encoder, 6M params) run locally. This means recall — the most frequent operation — has zero API cost. Consolidation and reflect are the only operations that need external LLM calls, and consolidation runs in the background where batch APIs cut costs by 50%.

## Practical Recommendations

**For cost-sensitive deployments:**
- Use batch APIs for consolidation (50% savings on the most LLM-intensive operation)
- Keep `consolidation_llm_batch_size` high (16–32) if your LLM provider has generous rate limits
- Invest in mental models for frequently asked questions — they amortize reflect costs
- Use local embedding and reranking models (default) to keep recall free

**For latency-sensitive deployments:**
- Tune `ef_search` for your HNSW indexes (higher = better recall, slower queries)
- Use `thinking_budget: low` for graph retrieval if you can tolerate less exploration
- Set `reflect_max_iterations` to 5–7 (from default 10) for faster synthesis
- Consider external reranker APIs if CPU is the bottleneck

**For quality-sensitive deployments:**
- Enable consolidation with a reasonable `max_observations_per_scope` to prevent observation bloat
- Use delta refresh for mental models to preserve curated content while keeping data-driven sections fresh
- Increase `thinking_budget` to HIGH for graph retrieval — the richer traversal improves recall quality at the cost of latency
- Over-fetch aggressively in semantic search (the 5x default is a good starting point)

**For horizontal scaling:**
- Multiple API instances behind a load balancer work out of the box — all state is in PostgreSQL
- Connection pooling (PgBouncer or equivalent) is critical once you're past a handful of instances
- Consolidation workers can run on dedicated instances to separate background work from user-facing traffic
- HNSW index builds are the most expensive migration operation — plan for them during low-traffic periods
