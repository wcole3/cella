# E6 — 4× time resolution · REJECTED (as tested)

_Round 1 (2026-08-15) · 1 seed · calibration fires · runner `exp_variants.py` · raw results not kept in `results/` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** Fire can move one cell (30 m) per tick, and there are 50
ticks per day, so the model's front cannot go faster than 1.5 km/day.
Real fires ran 10–30 km on their big days. We tried four times as many
ticks per day with p0 divided by four to keep the total spread roughly
the same. Scores went down on three fires and up on one. The p0 ÷ 4
compensation was too crude near the percolation cliff; a fair test needs
to re-scan p0 at each tick rate, which E11 did.

**Question.** Does removing the speed cap (more ticks per day) improve
the score?

**What we changed.** 200 ticks per day instead of 50, with p0 ÷ 4 as a
crude rate correction. E1 recipe otherwise.

**Why we expected it to matter.** The 1.5 km/day cap is far below
observed run-day speeds, so the model should be too slow on those days.

**How we scored it.** Change in mean IoU from the E1 recipe, one seed,
four calibration fires.

**Result.** Bear −0.05, Brattain −0.02, Buck +0.03, Chimney −0.02.

How to read it: negative is worse. One gain, one tie, two losses.

**What it means.** Nothing yet. Dividing p0 by four is not the right
correction: near the cliff a small p0 change swings the burned area
many-fold, so the comparison is between two different fires, not two
tick rates. At daily truth the "burns too much" error also swamps the
"too slow" error, so even a fair test may show little.

**Questions this raises.**

- What does a fair test (re-scan p0 at each tick rate) show? → E11/E11b:
  every fire moves by less than 0.03 from 50 to 400 ticks per day. The
  cap is not the binding limit.
- How fast does the model's front actually move per tick? → E19: 0.47–
  0.82 cells per tick at the E1 recipe, so the real ceiling is 0.7–1.2
  km/day, and one tick is not a defined length of time.

**Verdict.** Rejected as tested. Redone properly as E11.

**Later.** E11/E11b (cap not binding; p0 × ticks is one knob), E19 (tick
length should follow the weather), E22 (a wind-driven clock: invisible at
daily truth, harmful when it adds ticks).
