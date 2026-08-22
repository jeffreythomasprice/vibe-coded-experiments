TODO.md is for humans, don't update it, and try to avoid even referencing it unless specifically propmted

Prefer unit tests to prove a change works whenever possible.

When you do need to run the app to prove something, use short timeouts. The app
starts up and loads pretty much instantly, so a few seconds is plenty — don't
waste minutes on things like `timeout 240`.

For `lib::llm`, "prefer unit tests" means something specific: request
building, response/error parsing, and stream framing are all pure functions
over strings, tested against recorded fixtures in each provider's
`fixtures/` directory — no network involved. If you touch a provider's wire
format, add or update a fixture and a unit test rather than reaching for a
live call. The live tests in `lib/tests/` (`AI_HARNESS_LIVE=1 cargo test -p
lib --test live_<provider>`) actually call Anthropic, OpenAI, or a local
Ollama; Anthropic and OpenAI spend real money, so don't run them just to
check something works — run them when you need to confirm end-to-end
behavior against the real API, not as a substitute for a unit test.
