# Observed front speed vs the model's hard cap · finding (data)

_Round 2 (2026-09-01) · truth data only, no model runs · all six fires · results `obs_front_speed.json` · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** The model's front can advance at most one cell (30 m) per
tick, 50 ticks a day: 1.5 km/day. We measured how fast the real fronts
moved from the truth. Every fire broke the cap on some days, and on
Brattain, Chimney and Ferguson 80–98 % of all burned area arrived on
days that broke it. So the model is too slow on the days that matter,
then catches up by burning everywhere. This is the "too slow at the start, unstoppable at the end" shape problem
from ANALYSIS.md §5, quantified.

**Question.** How often, and by how much, do the real fires move faster
than the model can?

**What we measured.** For each observation window, the Euclidean distance
from the previous day's burned set to each newly burned cell; the maximum
daily advance; the number of windows whose advance exceeds 1.5 km; the
share of all new burned cells that arrived in those windows.

**Result.**

| Fire | max daily advance | windows over the cap | new cells in those windows |
|---|---|---|---|
| Bear | 2.3 km | 3 / 22 | 37 % |
| Brattain | 4.9 km | 10 / 21 | **98 %** |
| Buck | 2.3 km | 3 / 29 | 40 % |
| Chimney | 6.1 km | 8 / 15 | **80 %** |
| Ferguson (holdout) | 7.3 km | 18 / 29 | 87 % |
| Pier (holdout) | 3.4 km | 7 / 30 | 63 % |

How to read it: "windows over the cap" counts days when the real front
moved more than 1.5 km; the last column is how much of the fire's total
area arrived on those days. Bold marks the fires where nearly all the
burn came on such days.

**What it means.** Most of the real burned area arrives on days the model
*cannot* keep up with. It then catches up over the following days by
burning everywhere reachable. This also explains why the wind rule
cannot elongate the burn: elongation needs the downwind front to outrun
the flanks, but the downwind front is already pinned at the cap.

**Questions this raises.**

- Does raising the cap (more ticks per day) fix it? → E11/E11b: no, every
  fire moves by less than 0.03, because p0 × ticks is one knob.
- How fast does the model actually move per tick at the E1 recipe? →
  E19: 0.47–0.82 cells per tick, so the real ceiling is 0.7–1.2 km/day.
- Is the cap a problem for sub-daily truth? Yes, and it stays on the list
  for the GOFER and PT-FireSprd tiers (TEST_PLAN §2).

**Verdict.** Finding (data). E11 tests raising the cap.

**Later.** E11/E11b, E12 (the model is too round), E19, E22 (a clock that
adds ticks on windy hours only widens the fire), E37 (large elongated
fires are unreachable).
