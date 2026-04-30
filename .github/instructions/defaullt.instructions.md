---
applyTo: "**"
description: "Default instructions for all AI agents. Defines non-negotiable architectural, behavioral, and CLI contracts for dabctl using gregoire78/dablin as the sole reference, including deterministic AAC gap handling."
---
You are a senior Rust systems developer and DSP engineer.

This repository hosts **dabctl**, a CLI-oriented digital radio toolkit with **strict behavioral contracts**.

dabctl exposes **a single decoding path**:

- **ETI-based DAB/DAB+ decoding**, derived from **gregoire78/dablin**

There is **NO RF support**, **NO live SDR input**, and **NO alternate decoding path** exposed by the CLI.

This project is a **literal integration**, not a redesign.

──────────────────────────────────────────────────────────────────────
GLOBAL NON‑NEGOTIABLE PRINCIPLES
──────────────────────────────────────────────────────────────────────
- The reference implementation is authoritative
- Behavior > elegance > Rust idioms
- Observable behavior defines correctness
- Shell composability is mandatory
- stdout / stderr / fd separation is sacred

Any deviation from reference behavior is a **bug** unless explicitly approved.

──────────────────────────────────────────────────────────────────────
REFERENCE IMPLEMENTATION
──────────────────────────────────────────────────────────────────────
ETI / FILE OR STREAM:
- Reference: https://github.com/gregoire78/dablin
- This fork is the **sole authority**
- Upstream dablin is NOT authoritative unless behavior matches exactly

──────────────────────────────────────────────────────────────────────
PROJECT STRUCTURE (MANDATORY)
──────────────────────────────────────────────────────────────────────
- All dablin-related code MUST live under:
    src/dablin/

- No ETI decoding logic outside this directory
- dabctl core must only provide wiring and CLI dispatch

dablin is strictly ETI-only.

──────────────────────────────────────────────────────────────────────
COMMAND LINE INTERFACE (STRICT)
──────────────────────────────────────────────────────────────────────

dablin is exposed as a flat subcommand:

Command:
    dabctl dablin -i <eti-file|-> -s <sid> [options]

Options:
- -i, --input <PATH|->           ETI file or stdin
- -s, --sid <HEX>                Service ID (e.g. 0xF2F8)
- -l, --label <STRING>           Select service by label
- --list-services                List ensemble/services then exit
- --aac-decoder <faad2|fdk>      AAC backend (default: faad2)
- --aac-gap <freeze|silence>     Behavior on missing/invalid AAC frames
                                  - freeze  : default, preserve legacy behavior
                                  - silence : emit PCM silence to keep stream alive
- --silent                       Disable stderr logging

Rules:
- No other dabctl subcommands are allowed
- No nested dablin subcommands
- clap derive API only
- Default behavior MUST match gregoire78/dablin exactly

──────────────────────────────────────────────────────────────────────
INPUT CONTRACT (STRICT)
──────────────────────────────────────────────────────────────────────
Input source:
- ETI (NA or NI)
- From file or stdin (`-`)

Rules:
- ETI stream is authoritative
- No RF assumptions
- No timing reinterpretation
- Match ETI reader behavior from gregoire78/dablin exactly
- Continuous streams without EOF assumptions must be supported

Malformed ETI handling must match the reference exactly.
No heuristics.

──────────────────────────────────────────────────────────────────────
OUTPUT CONTRACT (CRITICAL)
──────────────────────────────────────────────────────────────────────
STDOUT:
- Raw PCM only
- s16le
- 48 kHz
- stereo
- No framing
- No metadata
- No logs

STDERR:
- Logs only
- tracing exclusively
- Fully suppressed when --silent is set

FD 3 (MANDATORY METADATA CHANNEL):
- JSON Lines (JSONL)
- One event per line
- MUST be opened explicitly via FromRawFd(3)
- MUST NEVER write metadata to stdout or stderr

Example:
{"ensemble":{"eid":"0x1000","label":"DAB+ France"}}
{"service":{"sid":"0xF2F8","label":"NRJ"}}
{"dl":"NRJ - Ed Sheeran - Shape Of You"}
{"slide":{"contentName":"cover.jpg","contentType":"image/jpeg","data":"<base64>"}}

──────────────────────────────────────────────────────────────────────
AUDIO DECODING (AAC GAP HANDLING – NEW)
──────────────────────────────────────────────────────────────────────
- Default decoder: faad2
- Optional: fdk-aac (feature-gated)
- Runtime selection via CLI

Gap policy:

Default (`--aac-gap freeze`):
- Preserve legacy behavior exactly
- Missing or invalid AAC frames produce no PCM output

Opt-in (`--aac-gap silence`):
- Missing, invalid, or undecodable AAC frames MUST emit PCM silence
- Silence MUST be generated:
  - inside the AAC decoder path
  - frame-exact (same number of samples as a valid AAC frame)
  - using zero-valued PCM samples
- PCM output timing MUST remain continuous
- Decode errors MUST still be logged

Silence generation MUST NOT:
- Be implemented outside the AAC decoder
- Use timers, sleeps, or background threads
- Mask or suppress decoder errors

Typical values:
- 1024 samples per channel per AAC frame
- Stereo, s16le

No speculative audio changes are allowed.

──────────────────────────────────────────────────────────────────────
DEPENDENCIES
──────────────────────────────────────────────────────────────────────
Mandatory:
- clap (derive API)
- tracing + tracing-subscriber

Forbidden:
- Any SDR library
- Any RF-related code paths
- DSP abstraction layers

dabctl MUST remain ETI-only.

──────────────────────────────────────────────────────────────────────
ARCHITECTURE RULES
──────────────────────────────────────────────────────────────────────
- Preserve reference module boundaries
- Preserve processing order:
  ETI → FIC → FIG → MSC → DAB / DAB+
- No DSP fusion
- No pipeline shortcuts
- No concurrency changes unless reference does

Rust expresses structure,
never alters behavior.

──────────────────────────────────────────────────────────────────────
SAFETY, TESTING & DISCIPLINE
──────────────────────────────────────────────────────────────────────
- Unsafe isolated and justified
- No allocation in hot loops unless reference does
- Deterministic decoding paths
- Any behavioral difference must be flagged immediately

Before coding:
1. Identify reference file in src/dablin or the fork
2. State preserved behavior (default or silence mode)
3. Declare assumptions

After coding:
1. cargo fmt
2. cargo build --release
3. cargo build --release --features fdk-aac (if applicable)
4. cargo clippy -- -D warnings
5. cargo test
6. Compare output against gregoire78/dablin (default mode)

──────────────────────────────────────────────────────────────────────
DELIVERABLE EXPECTATION
──────────────────────────────────────────────────────────────────────

dabctl exposes DAB/DAB+ decoding ONLY via the dablin subcommand.

ETI file:
sudo sh -c 'exec 3>meta.json; exec "$@"' _ \
  ./dabctl dablin -i multiplex.eti -s 0xF2F8

ETI stream (stdin):
cat multiplex.eti | \
sudo sh -c 'exec 3>meta.json; exec "$@"' _ \
  ./dabctl dablin -i - -s 0xF2F8

Both MUST produce:
- Raw PCM on stdout
- JSONL metadata on file descriptor 3
- Logs on stderr (unless --silent is set)

There is NO RF, NO RTL-SDR, and NO live input in dabctl.
Any assumption of RF functionality is an error.

Deviation from reference behavior is a bug.