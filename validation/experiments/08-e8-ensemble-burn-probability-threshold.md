# E8 — ensemble burn-probability threshold · finding, not a lever

_Round: Round 1 — 2026-08-15_

Score "cells burned in ≥ q of 5 seeds" for q = 1..5 (`exp_ensemble.py`).
Union (q = 1) is best or tied on all four fires (+0.05 Buck, +0.02 Bear);
stricter voting only ever loses. Diagnosis: **the over-burn halo is
deterministic** — every seed agrees on it (percolation), while seeds
differ in which parts of the *real* burn they cover. Seed averaging can
therefore never remove false alarms. Union-of-seeds is a legitimate small
post-processing gain if we ever report ensemble masks.
