"""OpenRouter GPT-4o vision backend: schematic image -> draft circuit.toml IR.

Stdlib-only HTTP (urllib) so this runs without torch/heavy deps — only an
``OPENROUTER_API_KEY`` is required. Mirrors SINA's VLM stage: the model performs
component detection, connectivity inference and designator reading in one
structured pass, constrained to our circuit.toml schema (see schema_spec).
"""

from __future__ import annotations

import base64
import json
import mimetypes
import os
import urllib.error
import urllib.request
from pathlib import Path

from .schema_spec import SCHEMA_SPEC

OPENROUTER_URL = "https://openrouter.ai/api/v1/chat/completions"
DEFAULT_MODEL = "openai/gpt-4o"

SYSTEM_PROMPT = (
    "You are a precise analog-circuit schematic digitizer. You convert a schematic "
    "image into a machine-readable netlist. You detect every component, infer the "
    "connectivity between their terminals, read reference designators and values, "
    "and emit a single TOML document in the exact schema given. Be conservative: if "
    "a value is illegible, use a plausible standard value and keep the topology "
    "faithful. Output ONLY the TOML."
)


def _image_data_url(path: Path) -> str:
    mime = mimetypes.guess_type(str(path))[0] or "image/jpeg"
    b64 = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:{mime};base64,{b64}"


def digitize(
    image_path: str | Path,
    *,
    model: str = DEFAULT_MODEL,
    api_key: str | None = None,
    extra_hint: str | None = None,
    timeout: float = 180.0,
) -> str:
    """Return the draft circuit.toml text produced by the VLM for ``image_path``.

    Raises RuntimeError if OPENROUTER_API_KEY is absent or the API call fails.
    """
    image_path = Path(image_path)
    key = api_key or os.environ.get("OPENROUTER_API_KEY")
    if not key:
        raise RuntimeError(
            "OPENROUTER_API_KEY not set — required for the VLM backend. "
            "Use the local backend (deps-gated) or export the key."
        )

    user_text = "Digitize this schematic into the circuit.toml schema below.\n\n" + SCHEMA_SPEC
    if extra_hint:
        user_text += f"\n\nADDITIONAL CONTEXT:\n{extra_hint}\n"

    payload = {
        "model": model,
        "temperature": 0,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": user_text},
                    {"type": "image_url", "image_url": {"url": _image_data_url(image_path)}},
                ],
            },
        ],
    }
    req = urllib.request.Request(
        OPENROUTER_URL,
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {key}",
            "Content-Type": "application/json",
            # OpenRouter attribution headers (optional but recommended).
            "HTTP-Referer": "https://github.com/gustavokch/vibe303",
            "X-Title": "vibe303 schemagen",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            body = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:  # pragma: no cover - network
        raise RuntimeError(f"OpenRouter HTTP {e.code}: {e.read().decode('utf-8', 'replace')}") from e
    except urllib.error.URLError as e:  # pragma: no cover - network
        raise RuntimeError(f"OpenRouter request failed: {e}") from e

    # OpenRouter (like the OpenAI API) often returns HTTP 200 with a top-level
    # {"error": {...}} body on rate-limit / bad-model / out-of-credit failures —
    # urllib does NOT raise for these. Surface the real message, not a raw dump.
    if isinstance(body, dict) and body.get("error"):
        err = body["error"]
        msg = err.get("message", err) if isinstance(err, dict) else err
        raise RuntimeError(f"OpenRouter error: {msg}")

    try:
        text = body["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as e:  # pragma: no cover - shape
        raise RuntimeError(f"unexpected OpenRouter response: {body}") from e
    return _strip_fences(text)


def _strip_fences(text: str) -> str:
    """Defensive: drop ```toml fences if the model adds them despite instructions."""
    t = text.strip()
    if t.startswith("```"):
        lines = t.splitlines()
        lines = lines[1:]  # drop opening fence
        if lines and lines[-1].strip().startswith("```"):
            lines = lines[:-1]
        t = "\n".join(lines).strip()
    return t + "\n"
