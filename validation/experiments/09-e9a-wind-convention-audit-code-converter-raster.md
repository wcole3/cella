# E9a — wind convention audit (code + converter + raster) · NO BUG

_Round: Round 2 — 2026-09-01: is the wind right, and why is the model too round?_

Three independent checks, all consistent:

1. **Code.** `wind_dir_deg` is the direction the wind blows *toward*,
   0° = +x, 90° = +y (down the grid). `dir_factors()` gives the
   neighbor at offset (−1, 0) — fire west of the cell — the maximum
   factor when θ = 0, so fire moves east. Unit test
   `wind_factor_is_max_downwind_min_upwind` pins this.
2. **Converter.** `convert_pytorchfire.py` maps ERA5 (u east, v north)
   to `atan2(−v, u)`, which is the same convention *if* raster row 0 is
   the north edge.
3. **Raster orientation.** Compared the LANDFIRE aspect layer (compass
   bearing of downslope, 0° = N) against the numerical gradient of the
   elevation layer on all six fires: mean cos = **+0.96** under
   "row 0 = north", ≈ 0 under "row 0 = south". Row 0 is north.

So the pipeline is self-consistent. The tester's expectation was the
meteorological convention (bearing the wind comes **from**, 0° = north,
clockwise). Conversion: `wind_dir_deg = (from_bearing + 90) mod 360` —
a north wind (from 0°) is `90` here. Now documented on the parameter
(rustdoc + the GUI panel description). Open question for the GUI: expose
a "from" bearing instead of the grid angle (see conclusions).
