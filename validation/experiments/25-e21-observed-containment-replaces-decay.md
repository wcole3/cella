# E21 — observed percent-contained (ICS-209) in place of the fitted decay · REJECTED — and it reframes what the decay is

_Round 3 (2026-09-02) · 3 seeds · calibration fires · runner `exp_containment.py` · results `exp21_containment.json` · data `data/ics209/` (gitignored, 49 MB) → `containment.json` per scenario · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** We had labelled the decay (E16, E17) a stand-in for firefighters. If
so, the real record should do at least as well: the daily percent-
contained that incident teams report. We fetched it for all six fires
and used it as the p0 multiplier instead of the decay. It did far worse.
Crews report 0–13 % containment in the first four days, so the real
record does nothing while the model runs away, whereas the decay has
already halved p0 by day 4. So the decay is not suppression. It is
standing in for something that slows these fires in their first days,
and we do not yet know what.

**Question.** Does the observed containment record reproduce the decay's
gain?

**What we changed.**

- **Data.** ICS-209-PLUS (St. Denis et al. 2023, Figshare, CC BY 4.0), the
  1999–2020 wildfire situation-report table:
  `PCT_CONTAINED_COMPLETED`, `ACRES`, `TOTAL_PERSONNEL` per daily
  report. All six fires found (Bear via the North Complex report it was
  merged into). Written to `scenario/containment.json` with provenance.
- **Method.** Per window p0 × (1 − C(t))^γ with C the cumulative-max,
  time-interpolated percent-contained, γ ∈ {1, 2}, p0 ×{2, 3, 4} (×{3, 4,
  6} on the hourly recipe). Two settings: ERA5 daily windows, and the E17
  hourly recipe with containment replacing the decay. Same-window fitted
  τ 5 decay as control.

| Fire | ICS-209 incident | reports in window | containment by day 4 / 7 / 10 / 15 |
|---|---|---|---|
| Bear (North Complex) | 2020_11865970 | 28 | 0 / 6 / 37 / 41 % |
| Brattain | 2020_11928423 | 25 | 0 / 15 / 30 / 87 % |
| Buck | 2017_7199245 | 28 | 13 / 18 / 35 / 30 % |
| Chimney | 2016_4367985 | 34 | 25 / 35 / 35 / 52 % |
| Ferguson (holdout) | 2018_9075048 | 59 | 5 / 7 / 26 / 29 % |
| Pier (holdout) | 2017_7385500 | 49 | 0 / 15 / 40 / 60 % |

How to read it: the last column is what the incident team reported as
contained at four points in time. Compare with the decay, which is
already at 45 % of its starting p0 by day 4.

**How we scored it.** Mean IoU with area ratio in brackets, 3 seeds, four
calibration fires.

**Result.**

| variant | Bear | Brattain | Buck | Chimney |
|---|---|---|---|---|
| **daily, decay τ5, p0 ×2 (control)** | **0.427** (×0.7) | **0.407** (×0.8) | **0.541** (×1.0) | 0.393 (×1.0) |
| daily, contain γ1, ×2 | 0.261 (×6.7) | 0.322 (×3.4) | 0.265 (×3.7) | 0.425 (×3.0) |
| daily, contain γ2, ×2 | 0.287 (×4.6) | 0.328 (×2.9) | 0.387 (×2.0) | **0.436** (×2.1) |
| daily, contain γ1–2, ×3–4 | 0.16–0.20 (×7–8) | 0.27–0.30 (×4) | 0.15–0.18 (×5–7) | 0.37–0.41 (×3) |
| hourly, moist × decay τ5, ×4 (control) | 0.418 (×0.8) | 0.384 (×1.0) | 0.502 (×0.8) | 0.390 (×0.7) |
| hourly, moist × contain γ2, ×3 | 0.331 (×2.8) | 0.327 (×2.4) | 0.485 (×1.0) | 0.433 (×1.6) |
| hourly, moist × contain γ1, ×3 | 0.295 (×4.5) | 0.317 (×3.0) | 0.396 (×1.7) | 0.440 (×2.4) |
| Circle | 0.541 | 0.450 | 0.670 | 0.372 |

How to read it: mean IoU with area ratio in brackets; bold is the best
row per fire. The two "control" rows are the fitted decay on the same
windows.

- **Real containment cannot cap the burn; the fitted decay can.** On
  every slow fire the observed schedule loses 0.1–0.28 to the τ 5 decay
  and the area explodes again (×3–8). Reported containment is 0–13 % for
  the first four days and under 40 % for ten, so (1 − C) ≈ 1 while the
  model does its damage, whereas e^(−t/5 d) is already 0.45 by day 4.
- Chimney, the fast fire, is the exception once more: observed
  containment (25 % by day 4) beats both the decay and control there
  (0.436 vs 0.393 / 0.441), because it removes area *late* rather than
  early.

**What it means.** Whatever e^(−t/τ) stands in for happens in the first
days and is not what the incident team calls containment. Candidates,
all testable: the model's own initial-attack gap (the ignition mask is
already a day-old fire); early-days fuel heterogeneity a uniform p0
ignores; run-day weather being front-loaded on these fires (E19: rate is
set by p0, so a fast first day can only be bought with a p0 that must
then decline); and the percolation cliff itself. The honest label for τ
is **an empirical early-days growth-rate decline**, not "crews".
Containment percent stays in the scenario as a legitimate observation:
the right target for a line agent (E18, E23) and a check on any suppression model,
just not a p0 multiplier.

**Questions this raises.**

- What does τ track? Open. Suggested: fit τ per fire and regress it on
  first-week weather, fuel and ignition-mask age.
- Is there a published stopping rule that behaves like the decay? → E28:
  a daily containment probability by growth rate does the decay's job
  inside the ensemble.
- Can the learned containment rate be checked against ICS-209? Open;
  listed as the next step after E28 and E31.

**Verdict.** Rejected as a p0 schedule. Kept as data. The fitted decay
stays, relabelled.

**Later.** E28 (containment operator), E31 (operator and decay are a tie
everywhere), E33/E34 (every filter run ends fully contained, which is the
operator's version of the plateau).
