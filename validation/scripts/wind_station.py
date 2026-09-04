#!/usr/bin/env python
"""Load real hourly weather observations (NOAA ISD) for each fire scenario.

Why: ERA5 daily domain-mean winds are too weak to act and, on Chimney 2016,
point the wrong way on the run days (experiments E9b/E9c). This pulls the
nearest airport/ASOS station's hourly record straight from NOAA's
Integrated Surface Database — no API key — and writes it next to the
scenario as `station_hourly.json`, in cella's own conventions:

- `from_deg`: compass bearing the wind blows FROM, 0 = north, clockwise
  (ISD already uses this; nothing to convert — see TEST_PLAN.md §2.1).
- `speed_ms`: 10 m (anemometer-height) wind in m/s. ISD stores m/s x 10.
- `temp_c`, `rh_pct`: from TMP and DEW (dew point -> RH via Magnus).
- `hours`: hours since the scenario's t0 (UTC), one row per hour, gaps
  linearly interpolated (direction interpolated as a vector).

Station choice: nearest ISD station whose file exists for the year and
that has good wind readings for >= 40 % of the hours (airports report
"calm" or missing often). Distance and coverage are recorded in the
provenance block so a comparison can be judged (TEST_PLAN §2.1).

Usage:
    validation/.venv/bin/python validation/scripts/wind_station.py [fire ...]
"""
import datetime as dt
import json
import math
import subprocess
import sys
from pathlib import Path

import h5py
import numpy as np
import pandas as pd

ROOT = Path(__file__).resolve().parents[1]
ISD = ROOT / "data" / "isd"
SCEN = ROOT / "data" / "scenarios"
FIRES = ["Bear_2020", "Brattain_2020", "Buck_2017", "Chimney_2016",
         "Ferguson_2018", "Pier_2017"]


def station_index() -> pd.DataFrame:
    p = ISD / "isd-history.csv"
    if not p.exists():
        ISD.mkdir(parents=True, exist_ok=True)
        subprocess.run(["curl", "-sSL", "-o", str(p),
                        "https://www.ncei.noaa.gov/pub/data/noaa/isd-history.csv"], check=True)
    h = pd.read_csv(p, dtype=str).dropna(subset=["LAT", "LON"])
    h["LAT"] = h.LAT.astype(float)
    h["LON"] = h.LON.astype(float)
    return h


def fetch_station_year(sid: str, year: str) -> Path | None:
    out = ISD / f"{year}_{sid}.csv"
    if not out.exists():
        url = f"https://www.ncei.noaa.gov/data/global-hourly/access/{year}/{sid}.csv"
        if subprocess.run(["curl", "-sSfL", "-o", str(out), url]).returncode != 0:
            return None
    return out


def parse_isd(path: Path, t0: dt.datetime, t_end: dt.datetime) -> pd.DataFrame:
    """Hourly table from one ISD station-year CSV, restricted to [t0, t_end]."""
    df = pd.read_csv(path, dtype=str, low_memory=False)
    df["time"] = pd.to_datetime(df.DATE)
    df = df[(df.time >= t0) & (df.time <= t_end)]
    wnd = df.WND.str.split(",", expand=True)
    dir_deg = pd.to_numeric(wnd[0], errors="coerce").where(wnd[0] != "999")
    spd = pd.to_numeric(wnd[3], errors="coerce").where(wnd[3] != "9999") / 10.0
    calm = wnd[2] == "C"
    spd = spd.where(~calm, 0.0)
    tmp = df.TMP.str.split(",", expand=True)[0]
    temp = pd.to_numeric(tmp, errors="coerce").where(tmp != "+9999") / 10.0
    dew = df.DEW.str.split(",", expand=True)[0]
    dewp = pd.to_numeric(dew, errors="coerce").where(dew != "+9999") / 10.0
    # Magnus formula: RH = 100 * e_s(Td) / e_s(T)
    rh = 100.0 * np.exp(17.625 * dewp / (243.04 + dewp)) / np.exp(17.625 * temp / (243.04 + temp))
    t = pd.DataFrame({"time": df.time, "dir": dir_deg, "spd": spd, "temp": temp, "rh": rh.clip(0, 100)})
    # Wind as a vector so hourly means and gap-filling do not average 350° and 10° to 180°.
    rad = np.radians(t.dir)
    t["u"] = -t.spd * np.sin(rad)   # eastward component of the wind (blows toward)
    t["v"] = -t.spd * np.cos(rad)   # northward component
    hourly = t.set_index("time").resample("1h").mean(numeric_only=True)
    full = pd.date_range(t0, t_end, freq="1h")
    hourly = hourly.reindex(full).interpolate(limit_direction="both")
    spd_h = np.hypot(hourly.u, hourly.v)
    from_h = (np.degrees(np.arctan2(-hourly.u, -hourly.v))) % 360.0
    return pd.DataFrame({"time": full, "from_deg": from_h.round(1), "speed_ms": spd_h.round(2),
                         "temp_c": hourly.temp.round(1), "rh_pct": hourly.rh.round(0)},
                        ).reset_index(drop=True)


def convert(fire: str, hist: pd.DataFrame, h5: h5py.File) -> None:
    g = h5[fire]
    b = g.attrs["bounds_in_4326"]
    lat, lon = (b[1] + b[3]) / 2, (b[0] + b[2]) / 2
    sc = json.loads((SCEN / fire / "scenario.json").read_text())
    truth = json.loads((SCEN / fire / "truth.json").read_text())
    t0 = dt.datetime.fromisoformat(sc["t0_utc"].replace("Z", ""))
    t_end = t0 + dt.timedelta(hours=truth["observed_at"][-1])
    year = str(t0.year)
    h = hist[(hist.BEGIN <= t0.strftime("%Y%m%d")) & (hist.END >= t_end.strftime("%Y%m%d"))].copy()
    h["dist"] = np.hypot((h.LAT - lat) * 111.0, (h.LON - lon) * 111.0 * math.cos(math.radians(lat)))
    n_hours = int(truth["observed_at"][-1]) + 1
    chosen = None
    for _, r in h.sort_values("dist").head(12).iterrows():
        sid = f"{r['USAF']}{r['WBAN']}"
        path = fetch_station_year(sid, year)
        if path is None:
            continue
        try:
            table = parse_isd(path, t0, t_end)
        except Exception:
            continue
        raw = pd.read_csv(path, dtype=str, low_memory=False)
        raw = raw[(pd.to_datetime(raw.DATE) >= t0) & (pd.to_datetime(raw.DATE) <= t_end)]
        good = (raw.WND.str.split(",", expand=True)[0] != "999").sum() if len(raw) else 0
        if good >= 0.4 * n_hours:
            chosen = (sid, r, float(good) / n_hours, table)
            break
    if chosen is None:
        print(f"{fire}: no usable ISD station within the 12 nearest", file=sys.stderr)
        return
    sid, r, coverage, table = chosen
    rows = [{"hours": float(i), "from_deg": float(x.from_deg), "speed_ms": float(x.speed_ms),
             "temp_c": None if pd.isna(x.temp_c) else float(x.temp_c),
             "rh_pct": None if pd.isna(x.rh_pct) else float(x.rh_pct)}
            for i, x in table.iterrows()]
    out = {
        "format_version": 1,
        "fire": fire,
        "station": {"isd_id": sid, "name": str(r["STATION NAME"]), "lat": float(r.LAT),
                    "lon": float(r.LON), "elev_m": str(r["ELEV(M)"]), "distance_km": round(float(r.dist), 1),
                    "raw_wind_coverage": round(coverage, 2)},
        "provenance": {
            "source": "NOAA Integrated Surface Database (ISD) global-hourly CSV",
            "source_url": f"https://www.ncei.noaa.gov/data/global-hourly/access/{year}/{sid}.csv",
            "retrieved": dt.date.today().isoformat(),
            "converter": "validation/scripts/wind_station.py",
            "weather": {
                "variables": "WND direction/speed, TMP, DEW (RH via Magnus)",
                "wind_height_m": 10.0,
                "native_resolution": "one station, hourly (often :53 past the hour); point value, not a field",
                "averaging": "hourly bin mean of the wind VECTOR; gaps interpolated as a vector",
                "source_direction_convention": "ISD: bearing the wind comes FROM, 0 = north, clockwise (true north)",
                "conversion": "none needed for from_deg; speed m/s = ISD tenths / 10",
                "grid_orientation_check": "inherited from scenario.json (row 0 = north, verified vs aspect)",
                "caveat": f"station is {r.dist:.0f} km from the fire centroid at a valley airport; "
                          "ridge-top winds on the fire can differ in speed and direction",
            },
        },
        "rows": rows,
    }
    (SCEN / fire / "station_hourly.json").write_text(json.dumps(out))
    spd = np.array([x["speed_ms"] for x in rows])
    print(f"{fire}: {sid} {r['STATION NAME']} {r.dist:.0f} km, coverage {coverage:.0%}, "
          f"{len(rows)} h, wind mean {spd.mean():.1f} max {spd.max():.1f} m/s")


def main() -> None:
    fires = sys.argv[1:] or FIRES
    hist = station_index()
    with h5py.File(ROOT / "data" / "dataset.hdf5") as h5:
        for fire in fires:
            convert(fire, hist, h5)


if __name__ == "__main__":
    main()
