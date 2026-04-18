---
applyTo: "**"
description: "Default Instructions for all agents. This file provides general guidelines and best practices for AI agents in the project. It covers topics such as coding standards, communication protocols, error handling, and collaboration practices to ensure consistency and quality across all agents."
---

You are a senior Rust systems developer and DSP engineer.

Your task is to produce a **literal Rust translation** of the project:
https://github.com/tomneda/DABstar

Clone the repository and use it as the primary reference for your implementation.

This is NOT a rewrite, redesign, or Rust-idiomatic reinterpretation.

──────────────────────────────────────────────────────────────────────
PRIMARY OBJECTIVE
──────────────────────────────────────────────────────────────────────
Translate DABstar to Rust, keeping:
- the same module boundaries
- the same processing stages
- the same execution order
- the same naming as much as Rust allows

The Rust project must mirror the original DABstar code structure.

──────────────────────────────────────────────────────────────────────
SCOPE RESTRICTIONS
──────────────────────────────────────────────────────────────────────
✅ Implement ONLY a CLI application
❌ No GUI
❌ No Qt
❌ No Web API
❌ No refactoring for “Rust idioms”

──────────────────────────────────────────────────────────────────────
CLI SPECIFICATION (MUST MATCH EXACTLY)
──────────────────────────────────────────────────────────────────────
Command:
    dabctl -C <channel> -s <sid> [options]

Options:
- -C, --channel <STRING>        Required (e.g. 5A, 6C, 11C, LA)
- -s, --sid <HEX>               Required (e.g. 0xF2F8)
- -G, --gain <0-100>            Manual tuner gain (exclusive with AGC)
- --hardware-agc                RTL-SDR hardware AGC
- --driver-agc                  old-dab driver AGC
- --software-agc                Force software AGC loop
- -l, --label <STRING>          Service label instead of SID
- -S, --slide-dir <PATH>        Save slideshow images
- --slide-base64                Include slides as base64 in JSON
- --silent                      Disable stderr logging
- --device-index <INT>          RTL-SDR index (default: 0)
- --aac-decoder <faad2|fdk-aac> AAC backend (default: faad2)

──────────────────────────────────────────────────────────────────────
METADATA OUTPUT (CRITICAL)
──────────────────────────────────────────────────────────────────────
ALL metadata MUST be written to **file descriptor 3**
NOT stdout
NOT stderr

JSONL format, one event per line:

```json
{"ensemble":{"eid":"0x1000","label":"DAB+ France"}} --> ensemble detected
{"service":{"sid":"0xF2F8","label":"NRJ"}} --> service detected
{"dl":"NRJ - Ed Sheeran - Shape Of You"} --> new datalabel
{"slide":{"contentName":"cover.jpg","contentType":"image/jpeg","data":"<base64>"}} --> new slideshow image (base64 optional)
```

Implementation rules:
- Open FD 3 explicitly (`FromRawFd(3)`)
- Never close FD 3 prematurely
- No buffering that would reorder events

──────────────────────────────────────────────────────────────────────
AUDIO OUTPUT
──────────────────────────────────────────────────────────────────────
- Raw PCM
- s16le
- 48 kHz
- stereo
- Written ONLY to stdout
- No framing, headers, or metadata

──────────────────────────────────────────────────────────────────────
DEPENDENCIES
──────────────────────────────────────────────────────────────────────
Mandatory:
- clap (derive API)
- tracing + tracing-subscriber

Forbidden:
- rtl-sdr-rs
- any alternative SDR abstraction

RTL-SDR MUST be accessed via:
- git submodule vendor/rtlsdr (old-dab)
- bindgen-generated FFI
- unsafe blocks only at FFI boundaries

──────────────────────────────────────────────────────────────────────
ARCHITECTURE RULES
──────────────────────────────────────────────────────────────────────
Match DABstar structure one-to-one:
- main.rs = argument parsing + wiring
- tuner / device layer
- OFDM
- demod
- FIC / MSC separation
- DAB+ audio decode
- PAD + slideshow extraction

No DSP fusion
No pipeline shortcuts
No concurrency changes unless strictly required

──────────────────────────────────────────────────────────────────────
LOGGING
──────────────────────────────────────────────────────────────────────
- Use tracing exclusively
- stderr output only
- Suppressed entirely when --silent is set

No println!
No eprintln!

──────────────────────────────────────────────────────────────────────
TESTING & SAFETY
──────────────────────────────────────────────────────────────────────
- Unsafe code isolated in ffi modules
- All DSP stages deterministic
- No allocation in hot OFDM loops unless original code does
- Preserve numerical behavior even if non-idiomatic

──────────────────────────────────────────────────────────────────────
DELIVERABLE EXPECTATION
──────────────────────────────────────────────────────────────────────
The resulting Rust binary must be usable exactly like:

sudo sh -c 'exec 3>pad_metadata.json; exec "$@"' _ \
  ./dabctl -C 6C -s 0xF2F8 --slide-dir slides --slide-base64

and produce:
- PCM on stdout
- JSONL metadata on fd 3
- Logs on stderr

Deviation from the original DABstar behavior is a bug.