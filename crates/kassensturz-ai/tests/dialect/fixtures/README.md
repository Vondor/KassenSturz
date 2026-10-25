# Provider dialect fixtures (M5.11)

What the four providers KassenSturz is tried with answer, as the
OpenAI-compatible adapter and the chat's tests read it (D-206):

| Folder | Provider | Written from |
|---|---|---|
| `openai/` | OpenAI (ChatGPT's API), `https://api.openai.com/v1` | OpenAI's chat completions reference |
| `claude/` | Claude's OpenAI-compatible endpoint, `https://api.anthropic.com/v1/` | Anthropic's "OpenAI SDK compatibility" page |
| `gemini/` | Gemini's OpenAI-compatible endpoint, `https://generativelanguage.googleapis.com/v1beta/openai/` | Google's OpenAI compatibility page and users' reports |
| `lmstudio/` | LM Studio's local server, `http://localhost:1234/v1` | LM Studio's documentation |

**These files are written by hand, not recorded from a live service.** No
key was used and no provider was contacted. All data is made up: the
"key" in the error bodies is the test key `sk-test-0000-made-up-key`, put
there to prove that no provider text and no key ever comes back out of an
error (threat T-25).

Shapes that a provider's documentation doesn't state and that come from
users' reports are marked here, and M5.B (real-provider rounds, D-172)
confirms or corrects them:

- `gemini/tool_call.json` and `gemini/parallel_calls.json`: the finish
  reason is `stop` although the model calls a function, and the calls may
  come without a usable ID (reported).
- `gemini/error_auth.json`: a bad key answered with HTTP 400 rather than
  401 (reported).
- `gemini/models.json`: model names with the prefix `models/`.
- `gemini/safety_blocked.json`: a blocked answer has no text.
- `lmstudio/error_no_model.json`: the error text for "no model loaded".
- `claude/error_overloaded.json`: Claude's HTTP 529.

`stream_ignored.txt` is what a server sends that ignores `"stream": false`
(server-sent events). To add a provider, add a folder with the same file
names and a `Dialect` in `../mod.rs`.
