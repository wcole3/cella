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
parameter values taken straight from the research literature.

One picture of the core problem before any numbers. Same fire, same final
day, three predictions:

![Three maps of the Bear 2020 fire side by side: the real burn is a small
compact shape of 51 square km; the model's prediction covers the entire
map, 402 square km; the Circle baseline is a plain disc of the correct
size.](figures/bear_triptych.png)

*The real Bear 2020 fire (left) burned 51 km². The model (middle) burned
essentially everything on the map — 402 km², stopped only by rivers and
roads. The Circle (right) knows no physics at all, but because its size is
forced to match reality it overlaps the real fire far better than the
model does. This is what "loses to the Circle" looks like.*

Full scorecard — higher is better; best score per fire in bold:

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
The same table as a picture:

![Horizontal bar chart of final overlap scores for all six fires. The grey
Circle bar is longer than the orange model bar on every fire except
Chimney 2016.](figures/final_scores.png)

And here is *where* the predictions go wrong, fire by fire. Dark cells are
land the model got right; orange is land the model burned that reality did
not (false alarms); blue is land that really burned but the model missed:

![Six maps, one per fire. In every map a dark correct region sits in the
middle of a much larger orange false-alarm region covering most of the
map. Blue missed areas are small fringes.](figures/agreement_maps.png)

*Two honest observations from these maps. The bad news: the sea of orange
is the over-burning — the model paints most of each map as burned. The
good news: the dark core shows the model does capture the real fire's
shape and location; the real burn is almost entirely inside the
prediction (very little blue). The model's problem today is knowing where
to* ***stop****, not where to start. Note Buck 2017's real perimeter has
straight edges — those are firefighter containment lines, physics the
model doesn't have (§5). Maps show one representative run (seed 0);
table scores are 3-run averages.*

## 5. What is wrong with the model today

**Shortcoming 1 — it burns far too much.** With textbook settings, the
model predicted roughly 400 km² burned on Bear 2020; the real fire burned
about 50 km². Eight times too much. This single problem drives most of the
bad scores: the 88% false-alarm rate *is* the over-burning. The textbook
values were tuned decades ago for a different setup, so this was expected
— it is why the test plan's first action item is calibration (tuning the
knobs on four fires, then verifying on two fires the tuning never saw).

![Six line charts, one per fire, of burned area versus time. On every
fire the orange model curve climbs far above the dashed reality curve
and keeps climbing after reality flattens out.](figures/area_curves.png)

*Watch the shape, not just the gap. Real fires (dashed) grow and then
flatten out — they run into damp fuel, cooler weather, and firefighters.
The model (orange) never flattens on its own; it climbs until it runs out
of land. On some fires (Ferguson, Brattain) reality actually spreads*
***faster*** *than the model early on, then the model blows past it — so
the model is both too slow at the start and unstoppable at the end. A
single speed knob cannot fix both directions at once; that is a shape
problem, not a tuning problem.*

**Shortcoming 2 — it sits on a knife edge.** Fire spread models of this
family have a known trap: below a threshold the simulated fire fizzles
out, above it the fire consumes everything reachable, and reality lives in
a narrow band between. We swept the main spread knob (`p0`, the base
chance that fire jumps to a neighboring cell) across its range on Bear
2020:

![Line chart of final burned area versus the p0 setting, on a log scale.
The curve is nearly flat and low up to 0.12, jumps almost vertically
between 0.12 and 0.20 — crossing the dashed reality line at about
0.15 — then flattens high.](figures/knife_edge.png)

*Between 0.12 and 0.20 — a small nudge of the knob — the burned area
jumps 35-fold, from "fizzled" (6 km²) to "exploded" (192 km²). Reality
sits on the cliff face. Small parameter changes swing results wildly,
which means calibration is genuinely hard, not a formality. This
sensitivity is itself a documented finding about this class of model.*

The sweep also produced a sharper finding: at p0 = 0.15 the model burns
almost exactly the right *amount* (45 km² vs the real 51 km²) — but its
overlap score is still only 0.12, far below the Circle's 0.52. **Even
with its size fixed, the model puts the burn in the wrong places on this
fire.** So calibration alone will not close the gap to the Circle;
better physics (below) has to carry some of the load. Knowing that
before the calibration campaign starts is exactly what these baselines
are for.

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

## 5a. What the September 2026 audit added

Two more things we now know, in plain words. Details and numbers are in
[EXPERIMENT_LOG.md](EXPERIMENT_LOG.md) Round 2.

**The wind arrow points the right way — but the wind we feed in is tiny
and sometimes wrong.** Someone testing the app expected "wind 0°" to mean
a north wind (weather-report style: the direction the wind comes *from*).
The model instead uses the direction the wind blows *toward*, with 0°
pointing right (east) on the grid. That second convention turned out to
be one we had made up, so we dropped it. The model now speaks
weather-report everywhere: you give it where the wind comes *from*
(0° north, 90° east), the same number a forecast or a weather station
prints, and it turns that into grid directions internally. Old files
that still use the made-up angle are refused with a clear error rather
than quietly read wrong. We checked the whole chain — the code, the file
converter, and whether the maps are stored north-up — and all three agree.

The real problem is upstream. The weather data we feed in (ERA5, one
average wind per day over the whole map) reports 0.1–3 m/s, a gentle
breeze that barely nudges the model: rotating that wind by 90°, 180°, or
switching it off changes the scores by less than the dice noise. And on
Chimney 2016 the newspapers describe hard easterly gusts driving the
fire west on the days it ran, while the daily average says a light wind
from the south-west. When we artificially strengthened the wind and
turned it to match the news, Chimney's score jumped from 0.30 to 0.57 —
the best any run has scored, and well above the Circle. So the wind
physics can work; it is being starved of a usable wind input. Even a
"cheating" run that borrows the real fire's growth direction each day
only helps one fire, because of the next point.

**The model has a speed limit, and real fires break it most days that
matter.** Fire can only jump one cell (30 m) per tick, and we run 50
ticks per day, so nothing can move faster than 1.5 km/day. Measured from
the satellite maps, real fronts advanced 2–7 km on their big days, and on
Chimney, Brattain, and Ferguson 80–98% of all burned land arrived on days
that broke the limit. A fire capped at walking pace cannot form a long,
wind-stretched shape: Brattain's real burn is 2.7× longer than wide, the
model's is a near-perfect disc.

We then tried the obvious fix — more ticks per day, up to 400 — and it
did **nothing** (every fire moved by less than 0.03). The reason is
worth understanding: in this model, the speed of the fire and the total
amount it burns are the same knob. Turn it up and the fire is faster
*every* day, so it also burns far too much. Real fires have a few
racing days and many stalled days. The model needs something that
changes day to day — weather — not a finer clock. The one such input we
have (a daily temperature proxy) is already worth +0.06 on Buck 2017
and never hurts; stronger day-to-day drivers (hourly wind, humidity)
are the next thing to build.

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
6. **Check the weather feed before believing a wind result — ours or
   anyone else's.** Wind can be written four ways (where it comes from
   or goes to; 0° at north or east; clockwise or not), in four units, at
   three heights, averaged over an hour or a day. Any one of those
   mismatches makes a correct model look wrong, and a wrong one look
   right. Every scenario now carries a `provenance.weather` note saying
   how its wind was measured and converted; a comparison with another
   model's published score needs the same note for *their* inputs. If
   that note is missing, the comparison is not yet a comparison.

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
