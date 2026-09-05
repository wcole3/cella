# Primer: genetic algorithms (search by breeding)

A plain-language introduction to genetic algorithms and their cousins,
novelty search and MAP-Elites, for someone who has never met them. It ends
with where each idea lives in `cella` (`docs/explore.md`), so the Evolve
side of the Explore tab reads as English.

## 1. The one idea

You have a machine with knobs and a way to score any setting of the knobs.
You want good settings. If the knobs were few and the score smooth you
would turn each one and watch the needle. But with ten knobs, a score that
jumps around, and a machine that takes a second per try, "turn and watch"
fails. A **genetic algorithm** (GA) borrows nature's answer:

> Keep a *population* of settings. Score them all. Let the good ones have
> children; children are copies with small random changes, sometimes mixing
> two parents. Repeat.

No gradient, no formula for the score, no assumption that the knobs are
independent. Just breeding and selection. The idea is John Holland's
(1975); the toolbox has not changed much since.

## 2. The vocabulary, with one example

Take Conway's Life on an 8×8 grid and ask: which birth and survival counts
give a grid that is about 30 % alive after 100 steps?

| Word | Meaning | In the example |
|---|---|---|
| **gene** | one knob | the birth count (0–8) |
| **genome** | one full setting of every knob | birth 3, survive 2–3 |
| **individual** | a genome plus its score | (3, 2, 3) → 0.31 alive |
| **population** | the set of individuals alive right now | 24 of them |
| **fitness** | the score, arranged so higher is better | `−|fraction − 0.30|` |
| **generation** | one round of score → select → breed | 30 of them |

The three operators that make a new generation:

- **Selection.** Pick parents, favouring the fit. *Tournament* selection
  picks the best of `k` random individuals (robust; it does not care about
  the scale of the score). *Roulette / Boltzmann* selection weights each
  individual by `e^(β × fitness)` (sharper as β grows).
- **Crossover.** Make a child from two parents: for each gene, take one
  parent's value or the other's (uniform crossover), or cut both genomes
  at one point and splice (one-point). The hope is that good *parts* of
  two solutions combine.
- **Mutation.** Nudge each gene of a child with some probability: a
  number moves by a random amount (σ, as a share of its range), a choice
  is redrawn, a bit flips. Mutation is what keeps finding new ground;
  selection is what keeps the ground gained.

Two more that every practical GA adds:

- **Elitism.** Copy the best few unchanged into the next generation, so
  the best score never goes down.
- **Immigrants.** Add a few fresh random genomes each generation so the
  population never fully converges on one idea.

That is the whole algorithm. `cella`'s defaults: population 24, elite 2,
crossover 0.5, mutation 0.3 per gene, σ 0.2, immigrants 0.1, tournament
of 3.

## 3. What goes wrong, and the knob that fixes it

- **Premature convergence.** After ten generations every genome looks the
  same, and it is not the best one; the population found a hill and
  climbed it, with nothing left to find other hills. Fixes: raise
  mutation or σ, add immigrants, use tournament rather than sharp
  Boltzmann selection.
- **Noise.** If the score depends on a random seed, a lucky genome looks
  good once. Fix: average the score over several seeds (`repeats`); and
  re-score the winner with seeds it never saw before believing it.
- **Overfitting the test.** A rule evolved on an 8×8 grid from one fixed
  starting picture may do nothing useful at 64×64 from a random start.
  Fix: evolve on random starts (a fresh one per generation, shared by the
  whole population so they compete fairly) and check the winner at the
  real size.
- **The trivial winner.** "Maximise how many cells are alive" is won by a
  rule that fills the grid on step one. Fix: a target instead of a
  maximum, a second measure (lifetime), or MAP-Elites (§5) so the boring
  winner is confined to its own corner.
- **Cost.** One generation = population × repeats × steps grid steps.
  Shrink the grid first, then the steps; the knobs you find usually
  transfer upward.

## 4. The famous CA example: evolving a rule that computes

Melanie Mitchell, Jim Crutchfield and Rajarshi Das (1990s) evolved 1D
cellular-automaton rules to solve **density classification**: start from
a random row of 0s and 1s, and end with all 1s if the row was mostly 1s,
all 0s otherwise. No single cell can see the whole row, so the rule has to
*compute* the majority by passing signals. The genome was the rule's
128-bit transition table; fitness was the share of random rows solved,
with fresh rows every generation. The GA found rules that send moving
"particles" across the row to carry the decision; nobody designed them.
`configs/1d_evolve_density_classification.json` is that experiment, and
`density_classification` is the metric.

## 5. Two cousins that do not optimise

A GA answers "what is the best setting?" Two later ideas answer a
different question: "what *can* this machine do?"

**Novelty search** (Lehman & Stanley, 2011) throws the objective away.
Fitness becomes *how different this individual's behaviour is from
everything seen so far*: measure a few features of the behaviour (how
active the grid is, how far things travel), keep an archive of behaviours
seen, and reward distance from the nearest neighbours. It sounds
perverse; it works because on hard problems the stepping stones to a good
solution often look nothing like it, and an objective-chasing GA walks
past them.

**MAP-Elites** (Mouret & Clune, 2015) keeps the objective but changes what
"population" means. Pick two or three behaviour features (the
**descriptors**), cut each into bins, and make a grid of cells, one per
combination. Each cell keeps the *single best genome whose behaviour
landed there*. Each generation: pick random elites, breed children, run
them, and file each child in its cell if it beats the occupant. The result
is not one winner but a **map of the best of every kind**, and the map
itself is the finding: which behaviours are reachable, which corners stay
empty, how fitness changes across the space. Mouret calls this
*illuminating* the search space; the family is *quality-diversity* (QD).

Two numbers describe an archive: **coverage** (filled cells / all cells)
and the **QD score** (sum of the elites' fitness), which rises both when
new cells fill and when existing cells improve.

For cellular automata this is the natural tool. Wolfram sorted rules into
four classes by eye (dies out, periodic, chaotic, complex). Run MAP-Elites
over Life-like rules with *activity × entropy* as the axes and *lifetime*
as the objective and the four classes appear as regions of the map, with
Life itself in the long-lifetime middle band
(`configs/2d_map_elites_life_classes.json`).

## 6. A GA inside a Monte Carlo filter

The particle filter in [primer-monte-carlo.md](primer-monte-carlo.md) §5
is a GA wearing a lab coat: weighting by score and resampling *is*
Boltzmann selection; the knob jitter *is* mutation; the fresh draws *are*
immigrants. The one difference is that a filter's children inherit their
parent's *state* (the grid as burned so far), not just its genome,
because the population is tracking a process through time rather than
searching a fixed landscape. `cella` uses one set of operators for both.

## 7. Where each word lives in `cella`

| Word here | In `cella` |
|---|---|
| gene, genome | `GeneSpec` / `Genome`; `"genes"` in the `"evolve"` block (`docs/explore.md` §4) |
| fitness | an `Objective` (metric + when + goal), or your own `Fitness` impl |
| population, generations, elite, crossover, mutation, σ, immigrants | the same-named fields of `"evolve"` |
| tournament / Boltzmann | `"selection": {"tournament": {"k": 3}}` / `{"boltzmann": {"beta": …}}` |
| repeats (noise) | `repeats` |
| random starts | `"initial": {"random": {...}}` |
| density classification | `"metric": "density_classification"` |
| novelty search | `"search": {"novelty": {"k": 15}}` |
| MAP-Elites, descriptors, coverage, QD score | `"search": {"map_elites": …}`, `"descriptors"`, `ArchiveStats`, the Archive gallery in the Explore tab |
| hall of fame | `Evolution::hall_of_fame`, "Best genome" in the GUI |

## 8. Sources, in order of gentleness

- MathWorks, *How the Genetic Algorithm Works*. The clearest one-page
  walk through elite, crossover and mutation children, with a diagram.
  <https://www.mathworks.com/help/gads/how-the-genetic-algorithm-works.html>
- Vemuri, *Genetic Algorithms Short Tutorial* (UC Davis ECS 271). The
  Darwinian framing and the classic operators in a few screens.
  <https://www.cs.ucdavis.edu/~vemuri/classes/ecs271/Genetic%20Algorithms%20Short%20Tutorial.htm>
- Mitchell, Crutchfield & Das, *Evolving Cellular Automata with Genetic
  Algorithms: A Review of Recent Work*. The density-classification story.
  <https://melaniemitchell.me/PapersContent/evca-review.pdf>
- Mouret, *Quality Diversity Algorithms* (lab page). One paragraph on
  what QD is, the MAP-Elites paper, and a notebook tutorial.
  <https://members.loria.fr/jbmouret/qd.html>
- Mouret & Clune, *Illuminating search spaces by mapping elites* (2015).
  The MAP-Elites paper; short and readable.
  <https://arxiv.org/abs/1504.04909>
- Lehman & Stanley, *Abandoning Objectives: Evolution through the Search
  for Novelty Alone* (2011). Why not chasing the objective can find more.
  <https://gwern.net/doc/reinforcement-learning/exploration/2011-lehman.pdf> (mirror; the journal copy is paywalled)
- Wikipedia, *Genetic algorithm*, for history and the many variants.
  <https://en.wikipedia.org/wiki/Genetic_algorithm>
