# Claudish

Translate between plain English and over-technical "Claudish" by using two
small [ProgramAsWeights](https://programasweights.com) programs, locally, and
shrink the prose that gets fed back to Claude Code.

rtk rewrites `git status` into `rtk git status` so the model reads a compact
result. Claudish does the same for the other big source of re-fed text:
subagent reports. When a subagent finishes, the hook translates its final
report from Claudish into plain English and the parent agent only ever reads
the short version.

**[Try the live demo](https://programasweights.com/claudish)**

## Install

```bash
cargo install --git https://github.com/pawann-2000/claudish-cli
claudish init -g        # patch ~/.claude/settings.json, download models (~620 MB, once)
```

`claudish init` (no `-g`) patches the project's `.claude/settings.json`
instead. `claudish init -g --uninstall` removes the hook. A `settings.json.bak`
is written before every change. Restart Claude Code afterwards.

Building compiles llama.cpp, so you need `cmake` and a C++ toolchain.

## Use

```bash
claudish to-english "Here's where I'd hold the line: do not launch until the tests pass. Green is the gate, not a suggestion."
# Do not launch until the tests pass.

claudish to-claudish "The release can go out after Alice approves the final report."

some-command | claudish to-english     # stdin works too, like `rtk pipe`
claudish gain                          # savings so far
```

The programs and the `qwen3-0.6b` base model download once into
`~/.cache/programasweights` (shared with the Python SDK; `PAW_CACHE_DIR`
overrides). A translation takes a few seconds including model load.
llama.cpp's device banner is hidden; set `PAW_VERBOSE=1` to see it.

Text goes through the program whole, exactly like the live demo, both from
the CLI and from the hook. Parentheticals, asides, and trailing notes get
dropped when the model judges them non-substantive.

## How the hook works

```
subagent finishes  ──SubagentStop──>  claudish hook claude
                                        │  translate last_assistant_message
                                        │  to plain English (local 0.6B model)
                                        └─ block once with the short text as the
                                           reason; the subagent re-emits it and
                                           the parent reads only that
```

Claude Code returns subagent results as a task notification rather than a
tool result, so `PostToolUse.updatedToolOutput` never sees them; blocking the
stop is the one seam that does. The hook lets a stop through when:

- `stop_hook_active` is set (the second stop, so it never loops),
- the report is under 240 characters,
- the translation is not at least 15% shorter.

The subagent pays one extra short turn; the parent's context, which is re-read
on every later turn, keeps only the compressed text.

## Regression test

```bash
cargo test --release -- --ignored   # runs tests/golden/*.in through the model
```

Each `.in` must translate byte-for-byte to its `.out`; local inference is
greedy with a fixed seed. Add a pair to pin a new case.

## Savings

`claudish gain` reads an append-only ledger at
`~/.local/share/claudish/history.jsonl`. Tokens are estimated as bytes / 4,
the same rough rule rtk uses: percentages are meaningful, absolute counts are
approximate.

## Specs

- [`specs/english-to-claudish.md`](specs/english-to-claudish.md)
- [`specs/claudish-to-english.md`](specs/claudish-to-english.md)

Copy the spec into your PAW program or adapt it for another model.

## Public programs

- [English → Claudish](https://programasweights.com/hub/ca9d5165b6c8e6615529)
- [Claudish → English](https://programasweights.com/hub/e469f61ccab2699fbd51)

Inspired by
[gvzdv/claudish-to-english](https://github.com/gvzdv/claudish-to-english) and
[rtk-ai/rtk](https://github.com/rtk-ai/rtk). This is an unofficial parody
project and is not affiliated with Anthropic.
