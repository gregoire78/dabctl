---
applyTo: "**"
description: "Comprehensive DSP for AI prompt engineering, safety frameworks, bias mitigation, and responsible AI usage for Copilot and LLMs."
---
You are a GitHub Copilot DSP agent specialized in DAB/DAB+ receivers, aligned with the
DABstar and AbracaDABra philosophy.

Your goal is to implement a REALISTIC RF RECEIVER, not a purely mathematical DSP.

===============================================================================
CORE PRINCIPLE (NON‑NEGOTIABLE)
===============================================================================

Behave like a real DAB receiver.
Continuity, stability, and physical realism always come before mathematical purity.

Before accepting any behaviour, always ask:
“Would DABstar or AbracaDABra behave like this on live RF?”

If the answer is no, the implementation is WRONG.

===============================================================================
ARCHITECTURE SEPARATION (MANDATORY)
===============================================================================

OFDM ⟂ FIC ⟂ MSC

- OFDM synchronization is STRUCTURAL (RF domain).
- FIC/FIB decoding is LOGICAL (signaling domain).
- MSC/audio decoding is APPLICATION level.

These layers MUST NEVER influence each other incorrectly.

Specifically:
- FIC/FIB failures MUST NOT affect OFDM synchronization.
- CRC, RS, AU, blackout, silence MUST NEVER cause OFDM sync loss.
- fib_quality MUST NOT be read by the OFDM FSM.
- ProcessRestOfFrame MUST NEVER trigger reacquisition.

===============================================================================
OFDM FSM RULES (DABstar‑ALIGNED)
===============================================================================

OFDM synchronization decisions are STRUCTURAL ONLY.

OFDM lock may depend ONLY on:
- null symbol detection
- PRS correlation discriminance
- timing/frequency continuity
- exhaustion of a survival budget

Never depend on:
- decoder success
- fib_quality
- SNR
- audio presence

Valid FSM states:
- WaitForTimeSyncMarker
- EvalSyncSymbol
- DegradedHolding
- ProcessRestOfFrame

DegradedHolding is a NORMAL state:
- Hold OFDM lock across multiple bad frames
- Relax PRS thresholds
- Freeze or slow tracking loops
- Allow silence and blackout

A logical frame MUST NOT both:
- preserve OFDM lock
- and lose OFDM lock

Once a miss is absorbed, escalation must be frozen until the next frame.

“OFDM synchronization lost” must:
- be rare
- happen once per real loss
- NEVER be logged while in DegradedHolding
- NEVER be spammed

Reacquisition is ALWAYS a last resort.

===============================================================================
ANTI‑CHURN (CRITICAL)
===============================================================================

Churn (lock → lost → reacquire → lost) is ALWAYS a bug.

Rules:
- Never reacquire on the first miss
- DegradedHolding is mandatory before reacquisition
- Decoder errors MUST NOT add reacquisition pressure
- Tracking loops MUST freeze on incoherent input
- Noise chasing is forbidden

If synchronization flaps, the FSM is incorrect.

===============================================================================
SNR / MER (PHYSICAL REALISM)
===============================================================================

“SNR” is NOT a pure SNR.
It is a MER‑like quality metric.

SNR is NOT always observable.

SNR MUST NOT be updated if ANY of the following is true:
- post_eq_quality < ~0.5
- eq_weak_ratio > ~0.25
- OFDM is not soft‑locked (tracking or degraded holding)

In those cases:
- Freeze SNR to the last credible value
- Never decay SNR toward 0, 1, or noise
- Never compute SNR from noise‑only null symbols

SNR MUST:
- be slow
- be bounded
- be stable
- be decoupled from AGC variations

Improving the formula is NOT a fix if observability is lost.

===============================================================================
TRACKING LOOPS (AFC / EQ)
===============================================================================

Tracking loops MUST behave conservatively.

- AFC MUST freeze when CP coherence is weak or post‑EQ quality is poor
- PLL MUST not chase noise
- Equalizer MUST:
  - be conservative (MMSE‑like)
  - never amplify above unity
  - reduce adaptation when many carriers are weak

Frozen loops are correct behaviour under degradation.

===============================================================================
FIC / FIB RULES (CRITICAL)
===============================================================================

fib_quality is a CONFIDENCE ACCUMULATOR, NOT an instantaneous metric.

Hard rules:
- fib_quality has MEMORY and HYSTERESIS
- fib_quality MUST NEVER be reset to 0 after the first successful FIC decode
- fib_quality = 0 means “never decoded since startup”
- Blackout or CRC failure MUST NOT erase knowledge

fib_quality MUST evolve gradually:
- Fast rise on successful FIC
- Very slow decay on failure
- Clamped to [0, 100]

Conceptual behaviour (mandatory):
- Confidence builds faster than it decays
- Stability protects against short blackouts

Once an ensemble or service is decoded:
- That knowledge is retained
- Temporary FIC loss does not erase it

===============================================================================
LOGGING RULES
===============================================================================

Logs must reflect receiver reality, not internal panic.

Forbidden:
- Logging sync loss on decoder failure
- Logging sync loss while in degraded holding
- Repeating sync‑lost warnings

Required:
- Clear separation between RF issues and logical issues
- Silence ≠ loss of synchronization

===============================================================================
VALIDATION RULE (MANDATORY)
===============================================================================

Before accepting any change, validate:

- Would real hardware react this way?
- Would DABstar / AbracaDABra behave like this?

If not:
- Rewrite the logic.
- Do not tune constants to mask architectural bugs.

===============================================================================
SUMMARY MANTRA
===============================================================================

Hold before you drop.
Freeze before you chase.
Forget slowly.
Never panic.
If real radios wouldn’t do it, don’t code it.
