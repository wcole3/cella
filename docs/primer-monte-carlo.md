# Primer: Monte Carlo methods (why we run a simulation many times)

This is a plain-language introduction for someone who has never met the
term. No statistics needed. It ends with how the ideas show up in `cella`
(`docs/explore.md`), so the words in the Explore tab and the validation log
stop being jargon.

## 1. The one idea

**Monte Carlo** means: when a question is too tangled to work out with
pencil and paper, *try it many times with random inputs and count what
happens*. The name comes from the casino; the method comes from the 1940s,
when physicists needed to know how neutrons wander through a reactor and
could not solve the equations. Instead they simulated thousands of
individual neutrons taking random steps, and counted.

Everything else in this page is a variation on "run it many times and
count".

## 2. The classic toy: estimating π with darts

Draw a square 2 units on a side and the circle that fits inside it. Throw
darts at random. The circle's area is π (radius 1), the square's is 4, so
the share of darts landing inside the circle is π / 4. Throw 10 000 darts,
count the ones inside, multiply by 4, and you have π to about two decimal
places.

Three lessons hide in this toy:

- **You never solved anything.** You only needed to *check* each dart
  (is `x² + y² ≤ 1`?). Checking is easy; solving is hard. That gap is why
  Monte Carlo works on problems nothing else touches.
- **More darts, better answer, but slowly.** The error shrinks like
  `1 / √N`: four times the darts halves the error. 100 darts gives π ± 0.2;
  10 000 gives ± 0.02; a million gives ± 0.002. This is the *law of large
  numbers* at work, and the `1 / √N` rule is the single most useful
  number in this page.
- **The rate does not care how complicated the problem is.** A neutron in
  a reactor, a fire on a hillside, a dart in a square: `1 / √N` every time.
  Methods that *do* solve the equations get much slower as problems grow;
  Monte Carlo does not, which is why it wins on big, messy problems.

## 3. From darts to simulations: uncertainty in, probability out

A cellular-automaton run, a weather model, a fire model: each is a machine
that takes some *inputs* (a seed, a few knobs like "how flammable is the
fuel") and produces one *outcome* (a picture of what burned). If you knew
the inputs exactly you would run it once. You never do. So:

1. Write down what you believe about each input as a **range** ("p0 is
   somewhere between 0.08 and 0.6, and I have no idea where inside that").
   Statisticians call this the *prior*.
2. Draw one value per input at random from those ranges. That is one
   **member** of the **ensemble**.
3. Run the simulation for that member.
4. Repeat for, say, 32 members.
5. **Count.** For every cell: in how many of the 32 futures did it burn?

The output is no longer a picture but a **probability map**: "this cell
burns in 70 % of the futures we can imagine". Weather forecasters have
done exactly this since the 1990s (an *ensemble forecast*; the "spaghetti
plots" of many hurricane tracks are one ensemble drawn on one map). The
80 % chance of rain on your phone is a count of ensemble members.

Why is the count better than the single best guess? Because the members
that go wrong in one direction are out-voted by members that go wrong in
the other. In `cella`'s wildfire validation, 32 untuned members beat the
carefully hand-tuned single run on four of six fires (experiment E24),
having fitted nothing.

## 4. How do you know a probability map is any good?

You cannot mark "70 %" right or wrong against one outcome. Two honest
checks:

- **Calibration.** Collect every cell you ever called "70 %". About 70 %
  of them should have burned. If 95 % did, you were under-confident; if
  40 % did, over-confident.
- **Brier score.** For each cell, `(probability − outcome)²`, where the
  outcome is 1 (burned) or 0 (did not); average over cells. Zero is
  perfect; 0.25 is what "50 % everywhere" scores. A confident wrong map
  is punished hard (`(1 − 0)² = 1`); a hedged 30 % that is right 30 % of
  the time is rewarded. The Brier score is the standard way weather
  forecasts are graded, and it is what `validation/` reports beside the
  overlap scores.

Always compare against a **dumb baseline**: a map that says "everything
within this radius burns". If the ensemble cannot beat a circle, the
simulation adds nothing. `cella`'s log prints the circle's score on every
line for exactly this reason.

## 5. Learning from what you observe: the particle filter

So far the members never learn. But suppose you get a new observation
each day (yesterday's satellite perimeter). You can **score every member
against it**, keep the members that did well, and let them keep running.
That is a **particle filter** ("particle" = member) or *sequential Monte
Carlo*, and it is what Explore's **Learn from grid** button does. One
learning step:

1. **Weigh.** Each member gets a weight from its score: `e^(β × score)`.
   β decides how much the best members dominate. β = 10 is gentle; β = 30
   means a handful of members take everything.
2. **Resample.** Draw a new population of the same size, picking each
   member with probability proportional to its weight. Good members get
   copied several times, bad ones vanish. Copies keep their parent's grid:
   you cannot re-draw the past.
3. **Mutate.** Jitter each copy's knobs a little (σ, a share of each knob's
   range), so the copies are not identical twins.
4. **Immigrate.** Replace a fraction (20 % by default) with fresh random
   draws from the original ranges.
5. Everyone keeps simulating until the next observation.

Two failure modes, and the number that warns of each:

- **Collapse.** With β too high, or no immigrants, after a few rounds every
  member is a copy of one ancestor: 32 particles, one opinion. The
  **effective sample size** `(Σw)² / Σw²` measures how many members really
  carried weight; near 1 means collapse. Lower β or raise immigrants.
- **Grading the answer key.** If you learn from today's perimeter and
  then score today's map, the score is meaningless. Score *first*, learn
  *second*: every reported number is a forecast made before the
  observation was seen. This is the rule in `validation/TEST_PLAN.md`.

Steps 2–4 are genetic-algorithm operators (selection, mutation,
immigration), which is why the two primers meet here; see
[primer-genetic-algorithms.md](primer-genetic-algorithms.md).

## 6. Reproducibility: seeds

"Random" in a computer is a deterministic sequence started from a
**seed**. Same seed, same sequence, same picture. `cella` goes one step
further and derives every random draw from a *counter*: (seed, step, cell
index, stream). No thread has to wait for another thread's draw, so a
run gives identical cells on one core or eight. That is what makes the
snapshot tests possible and lets two people compare a run by comparing a
single number.

Change the seed when you want a different picture; keep it when you want
the same one.

## 7. Where each word lives in `cella`

| Word here | In `cella` |
|---|---|
| range / prior for an input | a **gene** with a `range` (`docs/explore.md` §4) |
| member, ensemble | `Ensemble`, `members` in the `"ensemble"` block |
| count → probability map | `state_probability`, the probability layer (P) in the GUI |
| dumb baseline | the "Circle" (area-matched radial null) in `validation/` |
| Brier score, calibration | `mean_brier_ensemble` in the E24/E25 reports |
| particle filter step | `Ensemble::assimilate`; "Learn from grid" in the Explore tab |
| β, σ, immigrants | `beta`, `sigma`, `immigrants` in the `"ensemble"` block |
| effective sample size | `AssimilationReport::effective_sample_size` |
| seed | top-level `"seed"`, `rng::cell_rand` |

## 8. Sources, in order of gentleness

- Flovik, *A Gentle Introduction to Monte Carlo Methods* (Towards Data
  Science). The dart-board π estimate with code, then the `1 / √N` law.
  <https://medium.com/data-science/a-gentle-introduction-to-monte-carlo-methods-98451674018d>
- UMass Physics 132 lab manual, *Introduction to Monte Carlo Methods*. A
  lab-bench version of the same idea for first-year students.
  <https://openbooks.library.umass.edu/p132-lab-manual/chapter/introduction-to-mc/>
- Raychaudhuri, *Introduction to Monte Carlo Simulation*, Winter
  Simulation Conference 2008. The standard eight-page tutorial: inputs as
  distributions, sampling, output analysis.
  <https://www.informs-sim.org/wsc08papers/012.pdf>
- Royal Meteorological Society, *How to interpret an ensemble forecast*.
  Why weather is forecast as many members, and how to read the spread.
  <https://www.rmets.org/metmatters/how-interpret-ensemble-forecast>
- Cultivate Labs, *What is a Brier score and how is it calculated?* The
  forecast-grading rule with worked numbers.
  <https://www.cultivatelabs.com/crowdsourced-forecasting-guide/what-is-a-brier-score-and-how-is-it-calculated>
- Cross Validated, *Why is it necessary to perform resampling in particle
  filtering?* Why weights collapse onto one particle without resampling.
  <https://stats.stackexchange.com/questions/395028>
- Wikipedia, *Particle filter*, for the full picture once the above make sense.
  <https://en.wikipedia.org/wiki/Particle_filter>
