# E9a — wind convention audit (code + converter + raster) · NO BUG

_Round 2 (2026-09-01) · no runs; code and data checks · all six fires (raster check) · terms: [GLOSSARY.md](GLOSSARY.md)_

**In short.** A tester saw "wind direction 0°" push the fire east and
expected a north wind (weather-report style: the direction the wind comes
from). Before touching any parameter we checked the whole wind chain:
the model's rule, the converter that turns weather data into the
model's angle, and whether the maps are stored north-up. All three agree
with each other. The model used a home-made convention (direction the
wind blows *toward*, 0° = east); it was consistent, so no result in the
log was mis-winded. The convention was replaced the same day by the
weather-report one.

**Question.** Is the wind entering the model pointing the way we think?

**What we checked.**

1. **Code.** `wind_dir_deg` was the direction the wind blows *toward*,
   0° = +x, 90° = +y (down the grid). `dir_factors()` gives the neighbour
   at offset (−1, 0), the cell to the west, the maximum factor when
   θ = 0, so fire moves east. Unit test
   `wind_factor_is_max_downwind_min_upwind` pins this.
2. **Converter.** `convert_pytorchfire.py` maps ERA5 (u east, v north)
   to `atan2(−v, u)`, the same convention *if* raster row 0 is the north
   edge.
3. **Raster orientation.** The LANDFIRE aspect layer (compass bearing of
   downslope, 0° = N) against the numerical gradient of the elevation
   layer on all six fires: mean cos = **+0.96** under "row 0 = north",
   ≈ 0 under "row 0 = south". Row 0 is north.

How to read the raster check: cos +1 would mean the aspect layer and the
elevation gradient point the same way, cos 0 that they are unrelated.
+0.96 under one orientation and ≈ 0 under the other settles it.

**What it means.** The pipeline is self-consistent, so every score in
Round 1 stands. The tester's expectation was the meteorological
convention (bearing the wind comes **from**, 0° = north, clockwise). The
conversion is `wind_dir_deg = (from_bearing + 90) mod 360`: a north wind
(from 0°) was `90` in the old angle. The old convention was ours alone;
every weather source and operational simulator uses the from-bearing.

**Questions this raises.**

- Should the model take the weather-report bearing directly? → Done the
  same day (Round 2 conclusion 1): the parameter is now `wind_from_deg`
  everywhere, scenario format v2, old files rejected on load, every
  reported score unchanged because the conversion is exact.
- If the maths is right, is the wind *input* right? → E9b, E9c: no, it
  is too weak to act and wrong on Chimney's run days.

**Verdict.** No bug.

**Later.** TEST_PLAN §2.1 ("inspect the weather feed before you compare")
and process rule 6 came out of this audit.
