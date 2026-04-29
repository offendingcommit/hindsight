---
title: "Intelligent Document Parsing with LlamaParse and Hindsight"
authors: [benfrank241]
date: 2026-04-29T14:00:00Z
tags: [integrations, document-parsing, llamaparse, guide]
description: "Extract and parse complex documents with LlamaParse, turning PDFs and images into structured markdown that Hindsight can retain and recall for agent memory."
image: /img/blog/llamaparse-hindsight-integration.png
hide_table_of_contents: true
---

![Intelligent Document Parsing with LlamaParse and Hindsight](/img/blog/llamaparse-hindsight-integration.png)

If you're building agents that need to **parse and remember complex documents**, Hindsight now supports LlamaParse as a file parsing backend. LlamaParse converts PDFs, images, and other document types into clean, structured markdown—exactly the format Hindsight uses to extract and retain facts. This integration bridges the gap between raw documents and machine-readable memory.

<!-- truncate -->

## Why Document Parsing Matters for Agent Memory

Most agents struggle with documents because parsing and memory are separate problems. You extract text from a PDF, but then you lose the structure. You push markdown to an LLM, but the model has to re-parse what you already parsed. LlamaParse solves the first problem—turning messy documents into structured markdown—and Hindsight solves the second: extracting what's worth remembering from that markdown and keeping it accessible across sessions.

Together, they create a pipeline: document → structured markdown → extracted facts → persistent memory.

## How LlamaParse Works

LlamaParse is a hosted document parsing service from LlamaIndex. You upload a file (PDF, image, document), and it uses advanced vision and language models to understand the document's structure and content, then returns clean markdown.

The Hindsight integration handles the full workflow:
1. Upload the document to LlamaParse
2. Poll the service until parsing completes
3. Retrieve the markdown result
4. Pass it to Hindsight's retain pipeline for fact extraction

All of this happens transparently when you call `client.retain(document=file_bytes)` on a file type supported by LlamaParse.

## Setting Up LlamaParse with Hindsight

First, get a LlamaParse API key from [llamaindex.ai](https://llamaindex.ai). Then configure Hindsight to use it:

```bash
export HINDSIGHT_API_FILE_PARSER_LLAMA_PARSE_API_KEY=your-api-key
```

Or set it in your configuration:

```python
from hindsight_api import HindsightClient

client = HindsightClient(
    api_url="https://api.hindsight.vectorize.io",
    api_key="your-hindsight-key",
    file_parser_llama_parse_api_key="your-llamaparse-key"
)
```

Once configured, Hindsight automatically routes files to LlamaParse when needed. The service determines which file types it can handle, and Hindsight falls back gracefully if a type isn't supported.

## When to Use LlamaParse

LlamaParse excels with:
- **Complex PDFs**: Documents with tables, charts, or mixed layouts that naive text extraction mangles
- **Scanned images**: PDFs from scanning physical documents where OCR is critical
- **Visual documents**: Anything where the structure (headings, emphasis, layout) carries meaning
- **Multi-page documents**: Large documents where understanding structure helps extract the right facts

For simple text files or plain markdown, the built-in parser is usually enough. But when your agents need to parse a customer contract, a technical specification, or a scanned handbook, LlamaParse gives you production-quality extraction.

## Error Handling and Graceful Fallback

The integration distinguishes between two classes of failures:

- **Unsupported file types** (400/415/422): Hindsight returns a clear error and stops. You know the file type isn't supported.
- **Operational errors** (auth failures, timeouts, rate limits): These are logged and may trigger fallback strategies depending on your configuration.

This matters because you don't want to silently fail on an unsupported file type, but you also don't want to crash on a transient rate limit. The parser reports both clearly.

## Example: Retaining Facts from a Research Paper

```python
with open("research-paper.pdf", "rb") as f:
    response = client.retain(
        bank_id="research-bank",
        document=f.read(),
        tags=["research", "2026"]
    )
    print(f"Extracted {response.fact_count} facts from the paper")
```

Hindsight parses the PDF using LlamaParse, extracts structured facts, and stores them in your memory bank. Later, when you ask `client.recall("research-bank", "What was the key finding?")`, you get the facts Hindsight extracted—not raw text, not the full paper, just the signal.

## Next Steps

- [Hindsight Cloud](https://hindsight.vectorize.io)
- [File Parsing Configuration Guide](/developer/api/configuration)
- [Retain API Documentation](/developer/api/retain)
- [LlamaIndex Documentation](https://docs.llamaindex.ai/)
