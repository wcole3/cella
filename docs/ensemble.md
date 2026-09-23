# Ensembles (moved)

The wildfire-only ensemble described here until September 2026
(`WildfireEnsemble`, the `prior` block) has been replaced by a
model-agnostic engine that works on any rule or model. Everything is now in
[explore.md](explore.md):

- why one run is not enough, and the Monte Carlo / evolution / illumination
  decision table — §1–2
- configuring an `"ensemble"` block, learning from an observation — §7
- the wildfire driver (weather schedule, containment) as the worked example
  of a `MemberDriver` — §13
- translating an old `prior` block into `genes` — §16

The validation results that motivated ensembles (E24, E25, E28, and the E31
replication through the generic engine) are unchanged and live in
`validation/experiments/`.
