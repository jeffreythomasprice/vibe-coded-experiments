"""Agentic mode: the LLM gets a 'search_documents' tool it can call autonomously."""

from __future__ import annotations

import json
import litellm

from config import CHAT_MODEL, OLLAMA_BASE_URL
from query import retrieve


# ── Tool definition (OpenAI function-calling schema) ────────────────────

SEARCH_TOOL = {
    "type": "function",
    "function": {
        "name": "search_documents",
        "description": (
            "Search the local document store using semantic similarity. "
            "Use this to find information from ingested documents."
        ),
        "parameters": {
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural-language search query.",
                },
                "top_k": {
                    "type": "integer",
                    "description": "Number of results to return (default 5).",
                    "default": 5,
                },
                "tags": {
                    "type": "object",
                    "description": (
                        "Optional key=value tag filters. "
                        'Example: {"project": "alpha", "filename": "report.pdf"}'
                    ),
                    "additionalProperties": {"type": "string"},
                },
            },
            "required": ["query"],
        },
    },
}

TOOLS = [SEARCH_TOOL]


def _execute_tool(name: str, arguments: dict) -> str:
    """Run a tool call and return the result as a string."""
    if name == "search_documents":
        results = retrieve(
            query=arguments["query"],
            top_k=arguments.get("top_k", 5),
            tags=arguments.get("tags"),
        )
        return json.dumps(results, indent=2, default=str)
    return json.dumps({"error": f"Unknown tool: {name}"})


def agent_chat(user_message: str, system_prompt: str | None = None) -> str:
    """
    Run an agentic conversation loop.

    The LLM can invoke the search_documents tool as many times as it
    needs before producing a final text answer.
    """
    if system_prompt is None:
        system_prompt = (
            "You are a helpful research assistant with access to a local document "
            "store. Use the search_documents tool to find relevant information "
            "before answering. You may call the tool multiple times with different "
            "queries or tag filters. Cite source documents when possible."
        )

    messages = [
        {"role": "system", "content": system_prompt},
        {"role": "user", "content": user_message},
    ]

    # Agentic loop: keep going until the model produces a text-only response
    for _ in range(10):  # safety cap
        response = litellm.completion(
            model=CHAT_MODEL,
            messages=messages,
            tools=TOOLS,
            api_base=OLLAMA_BASE_URL,
        )

        choice = response.choices[0]

        # If no tool calls, we're done
        if not choice.message.tool_calls:
            return choice.message.content

        # Process each tool call
        messages.append(choice.message.model_dump())

        for tool_call in choice.message.tool_calls:
            fn = tool_call.function
            args = json.loads(fn.arguments) if isinstance(fn.arguments, str) else fn.arguments
            result = _execute_tool(fn.name, args)
            messages.append(
                {
                    "role": "tool",
                    "tool_call_id": tool_call.id,
                    "content": result,
                }
            )

    return "Agent reached maximum iterations without a final answer."
