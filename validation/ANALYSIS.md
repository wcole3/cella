# How to Read the Wildfire Validation Results

This is the plain-language companion to [TEST_PLAN.md](TEST_PLAN.md). You do
not need to know Rust, cellular automata, or statistics to read it. If you
only remember one thing, make it this:

> **Right now the model burns far too much land, and on 5 of our 6 test
> fires its map is worth less than simply drawing a circle of the right
> size. That is the honest starting point everything else gets measured
> against.**

That sounds bad — and it is the point. Our rule is that the goal is the
best answer, not a flattering score. A shortcoming we can see and measure
is a to-do list. A shortcoming hidden by a friendly metric is a landmine.

---

## 1. What we are actually testing

We have a simulator that tries to predict how a wildfire spreads across a
landscape, hour by hour. To find out whether it is any good, we replay six
real fires from recent US history (2016–2020). For each one we give the
simulator exactly what was known at the start — where the fire began, the
terrain, the vegetation, and the wind each day — and let it predict the
spread. Then we compare its predicted burn map against what actually
burned, day by day, using satellite observations.

The simulator rolls dice internally (real fire spread is partly chance),
so we never judge a single run. We run it several times with different
dice ("seeds") and score the average.

## 2. The scores, in plain words

Every score compares two maps: the area the model *predicted* would burn,
and the area that *really* burned.

**Overlap score (IoU)** — the headline number, between 0 and 1. Take the
two maps, measure the land they agree on (burned in both), and divide by
the total land either map marks as burned. Identical maps score 1.0; maps
that never touch score 0.0. For intuition: two equally-sized maps that
share two-thirds of their area score 0.5. This is the standard score in
published wildfire papers, so we can compare against other people's
models. (You will also see **Sørensen** in the raw output — a close cousin
of IoU that weighs overlap slightly more generously. It rises and falls
with IoU; if you understand IoU you can ignore it.)

One overlap number hides *which way* a prediction is wrong, so we also
track the two failure directions separately:

- **Miss rate** — of the land that really burned, what fraction did the
  model fail to predict? Misses are the dangerous direction: a miss is a
  neighborhood nobody warned.
- **False-alarm rate** — of the land the model predicted would burn, what
  fraction never actually did? False alarms erode trust and waste
  resources. Our current model's false-alarm rate is about 0.88 — 88% of
  what it paints as burned did not burn.

**Arrival-time error** — for land that both maps agree burned, how many
hours off was the model about *when* the fire got there? A model can draw
the right final map but be uselessly late or early at every point along
the way; this catches that.

## 3. The two dummy forecasters — our honesty floor

Scores mean nothing in a vacuum. Is 0.28 good? To answer that, every
single scoring run also scores two deliberately brainless "models" on the
same fire. They are built into the harness and cannot be switched off.

**Dummy #1 — "Persistence."** Predicts the fire never spreads at all: the
burn map stays frozen at the starting ignition. This is the floor of
floors. Any model scoring below persistence is actively destroying
information.

**Dummy #2 — "The Circle."** Draws a simple disc spreading evenly outward
from the ignition point — no wind, no terrain, no vegetation, no physics
of any kind. The one thing we let it cheat on: at each observation time we
tell it the *total area* that really burned, and it sizes its circle to
match exactly.

The Circle is the test that matters. Because its total area is always
perfectly right, the only way to beat it is to know *where* the fire went,
not just *how much* burned. All of the model's physics — wind response,
slope, fuel types — exists precisely to answer "where." **If the model
cannot beat the Circle, its physics is currently adding nothing over a
child's crayon guess.** That is a harsh bar, and it is exactly the bar an
honest evaluation needs.

## 4. The current scorecard (uncalibrated, August 2026)

These are the out-of-the-box results, before any tuning — textbook
parameter values taken straight from the research literature. Higher is
better; best score per fire in bold.

| Fire | Model | Persistence | The Circle | Verdict |
|---|---|---|---|---|
| Bear 2020 | 0.12 | 0.04 | **0.52** | loses to the Circle |
| Brattain 2020 | 0.20 | 0.01 | **0.44** | loses to the Circle |
| Buck 2017 | 0.14 | 0.14 | **0.62** | ties persistence — worst result |
| Chimney 2016 | **0.28** | 0.05 | 0.26 | **beats the Circle** |
| Ferguson 2018 | 0.29 | 0.00 | **0.36** | loses to the Circle |
| Pier 2017 | 0.22 | 0.14 | **0.54** | loses to the Circle |

How to read a row: on Bear 2020, the model's map overlapped the real burn
with a score of 0.12, while the brainless Circle scored 0.52 — the model's
physics made its prediction four times *worse* than guessing a circle.

## 5. What is wrong with the model today

**Shortcoming 1 — it burns far too much.** With textbook settings, the
model predicted roughly 400 km² burned on Bear 2020; the real fire burned
about 50 km². Eight times too much. This single problem drives most of the
bad scores: the 88% false-alarm rate *is* the over-burning. The textbook
values were tuned decades ago for a different setup, so this was expected
— it is why the test plan's first action item is calibration (tuning the
knobs on four fires, then verifying on two fires the tuning never saw).

**Shortcoming 2 — it sits on a knife edge.** Fire spread models of this
family have a known trap: below a threshold the simulated fire fizzles
out, above it the fire consumes everything reachable, and reality lives in
a narrow band between. On Bear, one setting burned 5,000 cells (fizzled),
a slightly higher one burned 212,000 (exploded) — the real fire's 56,000
sits in the gap. Small parameter changes swing results wildly, which means
calibration is genuinely hard, not a formality. This sensitivity is itself
a documented finding about this class of model.

**Shortcoming 3 — missing real-world physics.** The model currently does
not know about: **fuel moisture** (a damp forest spreads slowly no matter
the wind — likely why slow, long fires like Buck score worst, with the
model 186 hours off on arrival times); **firefighters** (real fires stall
at containment lines crews build — the model has no concept of
suppression, so late in a fire it keeps spreading where the real fire was
stopped); **local wind** (we feed one average wind for the whole map,
but canyons channel and redirect wind); and **ember spotting** (embers
igniting new fires far ahead of the front — the code supports it but it is
untuned and off). These are not oversights; they are deliberately staged
future work, and the tests are designed to measure how much each one hurts
before we build it.

**The bright spot — wind direction is real signal.** The one fire the
model beats the Circle on, Chimney 2016, was the fastest, most
wind-driven fire in the set. When a fire's shape is dictated by strong
wind, knowing the wind direction beats knowing nothing — evidence the
model's wind physics contributes something even before tuning.

## 6. Reading any future results table — a checklist

1. **Compare the model to the Circle first.** Beating persistence means
   almost nothing. Beating the Circle is the only evidence of real
   spatial skill.
2. **Look at all the fires, not the best one.** Our rules require every
   fire to be reported, including failures. A model that shines on one
   fire and fails five is a failing model. Averages across several dice
   rolls, never a lucky single run.
3. **Check which fires the number comes from.** Four fires (Bear,
   Brattain, Buck, Chimney) are used for tuning. Two (Ferguson, Pier) are
   *holdout* — the model is never tuned on them, so they are the only
   unbiased test. **A tuning improvement only counts if the holdout
   fires improve too.** Better tuning-fire scores with flat holdout
   scores means we memorized four fires, not learned fire behavior.
4. **Mind the two failure directions.** A model can inflate its overlap
   score by burning everything (few misses, huge false alarms). Miss rate
   and false-alarm rate keep it honest.
5. **Note the truth's own error.** Satellite burn maps have limited
   resolution (375 m for this dataset — carried in every report). No
   score should be read as more precise than the data behind it.

## 7. The rules that keep us honest

Written down *before* results were collected, in [TEST_PLAN.md](TEST_PLAN.md):

- The tuning/holdout split is fixed and may never be reshuffled to move a
  badly-scoring fire out of the holdout.
- The dummy forecasters run automatically inside every scoring pass —
  they cannot be forgotten or quietly dropped.
- All fires, all seeds, all metrics get reported. No cherry-picking.
- Observed truth data is never edited to fit the model.
- Negative results — failed tunings, worse-than-Circle configurations —
  are kept in the record, not deleted.
