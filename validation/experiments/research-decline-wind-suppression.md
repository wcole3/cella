# Research notes — why fires slow down, what wind a fire really feels, and what suppression does (2026-09-04)

Question from the user after Round 4: our containment decay `p0 × e^(−t/τ)`
works but is ad hoc. The field has studied all three of the things it
stands in for. What do they actually do, and what can we build?

Sources were read with firecrawl on 2026-09-04; each claim below names its
paper. Where a paywalled paper only gave an abstract, that is said.

## 1. How the field makes simulated fires stop

**FSim (Finney et al. 2011, *Stoch. Environ. Res. Risk Assess.* 25:973)**
— the US national large-fire simulator — does it with two rules, neither of
which is "the fire gets tired":

1. **Spread-event days.** Fire spread is simulated only on days when the
   Energy Release Component (a dryness index) is above its 80th
   percentile, and even then only for a *burn period* of 1, 3 or 5 hours
   at the 80th / 90th / 97th percentile. Podur & Wotton (2011, *IJWF*
   20:497) found the same for Canadian fires: nearly all growth happens on
   "spread event days" (ISI ≥ 7.5 or FWI ≥ 19); on other days the fire is
   treated as not growing.
2. **Containment probability.** A statistical model (Finney, Grenfell &
   McHugh 2009) gives the daily probability that a fire is *contained* as
   a function of its recent growth rate, its duration and the fuel type —
   slow-growing fires in non-timber fuels get contained; fast ones do not.
   Each simulated day the fire is stochastically terminated with that
   probability. A 2025 successor, "A generalized wildfire containment
   algorithm" (*Ecological Modelling* 505:111134), fits the daily proportion
   contained as a function of the fraction of the fire's duration elapsed
   and one "suppression factor", straight from ICS-209 reports — the same
   data we loaded in E21.

What this says about our decay: `e^(−t/τ)` is doing two jobs badly. The
spread-event rule is *periodic and gated*: the fire does not slow, it
stops on non-event days and races on event days. The containment rule is
*stochastic and growth-dependent*: the fire is more likely to be stopped
the slower it grew yesterday. Neither is a smooth exponential in time.

**What E15 got wrong about periodicity.** E15 damped p0 by a moisture
factor (0.1–1) and found it could not cap the burn. FSim's version gates
spread to zero outside the burn period, which *can* cap it if event days
are rare. On our California fires nearly every day is an event day, so the
gate alone would not have stopped them either — which is why FSim needs
the second rule.

**Retardant and fire line** (Giménez et al. 2004, *IJWF* 13:1; PROPAGATOR,
Trucchia et al. 2020): long-term retardant is modelled either as raised
fuel moisture (PROPAGATOR sets treated cells to moisture 0.8, i.e. almost
inert), as a reduced reaction intensity (Rothermel & Philpot 1975), or as a
fuel-model change. Effective coverage is 0.28–5.3 L/m² depending on fuel;
the effect persists until rain but degrades as the coating breaks. In a
cell model that is a per-cell *multiplier* that can be painted and later
restored — now `WildfireModel::set_density` — rather than a type change
that cannot be undone. E23's result (a veg-factor-0.1 line is ignored,
Inactive strangles) suggests the operationally realistic multiplier is far
below 0.1 for a fresh drop and rises back toward 1 over a day or two.

## 2. What wind a fire feels

**Downscaling** (Forthofer, Butler & Wagenbrenner 2014, *IJWF* 23:969;
Wagenbrenner et al. 2016, *ACP* 16:5229 — WindNinja): a mass-conserving
diagnostic model takes one wind (a station or a 3–30 km forecast cell)
and adjusts it as little as possible so that air is conserved over the
terrain. It reproduces ridge speed-up and valley channelling in seconds;
it does not reproduce lee-side recirculation or stable-layer decoupling
(no momentum equation, neutral stability assumed). Validation against
ridge stations: speed RMSE 1.6–2.7 m/s, direction RMSE 65–81° — better
than the raw forecast on ridgetops, still large. Their conclusion, and
ours from E14: **a valley station is not the wind on the ridge**, and
downscaling gets you part of the way. Implemented today as
`cella_lib::wind_field::mass_consistent` (the two-dimensional, single-layer
form of the same idea) feeding the new per-cell wind field in the model.

**Fire–atmosphere coupling** (Coen et al. 2013, *JAMC* 52:16, WRF-Fire;
also CAWFE, WRF-SFIRE): the fire changes its own wind. Fire-induced winds
of several m/s reach 5 km from the fire; the coupling blows the head
forward, runs parallel to the flanks and draws air *into* the fire across
the heel, which is what produces the bowed head and the narrow, fast
shape in high wind — the elongation our kernel lacks (E12, E19). Coen's
own verdict on our situation: "in contrast with the hypothesis that
surface weather stations and diagnostic surface wind models sufficiently
indicate the winds driving a nearby fire … the complex time- and
space-varying nature of weather has an important, perhaps dominant,
impact." A full coupled model is a research programme (WRF at LES
resolution); the cheap version worth trying is a *parameterised indraft*:
add to each burning cell's neighbourhood a wind component pointing from
the unburned side into the fire, scaled by the local burning-cell
density, which reproduces "flanks parallel, head forward, heel drawn in"
qualitatively.

**Wind exponent.** Rothermel-type spread grows roughly as wind^1.5–2
(Coen 2013 cites 0.4–2.7 across experiments; CFD models give a ~linear
rise from 0.4 to 1.5 m/s spread for 1–5 m/s wind). Our kernel's speed
response is 5–10 % over that range (E19). Any wind-field work is wasted
until the kernel's rate responds to wind.

## 3. What to build, in order

| # | Experiment | Mechanism from the literature | Status |
|---|---|---|---|
| E26 | terrain wind field | mass-consistent downscaling (WindNinja-style) → per-cell wind factors | code in place (`wind_field`, `set_wind_field`), experiment next |
| E27 | painted retardant | per-cell multiplier that decays back to 1 (Giménez; PROPAGATOR μ_wl) | code in place (`set_density`), experiment next |
| E28 | containment probability | Finney 2009 / 2025 algorithm: daily stochastic termination by growth rate, inside the ensemble | design below |
| E29 | spread-event gating | FSim burn period: zero spread outside event hours, hours from a dryness index | needs hourly RH/T (we have) and an ERC/FFWI proxy |
| E30 | kernel rate response | make front speed follow wind^1.5 (refit c1, or an elliptical/Huygens rule) | prerequisite for E22 to mean anything |
| — | fire-induced indraft | parameterised coupling (Coen) | after E30 |

**E28 design.** Each ensemble member tracks its own daily growth
`g = new cells / previous burned`. At the end of each day it is contained
with probability `P = 1 / (1 + exp(−(a + b·ln g + c·d)))` with `d` the
fire's age in days; contained members set p0 to 0 and stay as they are.
`a, b, c` are ensemble parameters with a prior, learned by the filter like
the others, so the ICS-209 record can be used to *check* the learned
containment rate rather than to fit it. This replaces τ with a mechanism
that has a published shape and a measurable output.
