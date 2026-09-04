#!/usr/bin/env python
"""E28: containment probability (FSim-style) inside the assimilating ensemble.

Each day every member is stochastically contained with
P = sigmoid(a + b ln g), g = its own relative growth that day; contained
members stop. a, b are member parameters (prior a in [-6,-1], b in
[-2,-0.3]) learned by the filter. Three configs, all assim, beta 10,
sigma 0.2, immigrants 0.2, M 32, all six fires:
  base           decay prior as in E25 (tau 2-100 d), no containment
  contain        decay prior + containment operator
  contain_tauoff containment only (tau fixed off): the physical replacement
"""
import json, sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
import exp_smc as base  # noqa: E402

common = {"SMC_BETA": "10", "SMC_SIGMA": "0.2", "SMC_IMMIGRANTS": "0.2"}
base.CONFIGS = {
    "e28_base": ("assim", dict(common)),
    "e28_contain": ("assim", {**common, "SMC_CONTAIN": "1"}),
    "e28_contain_tauoff": ("assim", {**common, "SMC_CONTAIN": "1", "SMC_TAU_OFF": "1"}),
}
base.M = 32
base.OUT = base.EXP / "exp28_containment_op"

if __name__ == "__main__":
    base.OUT.mkdir(parents=True, exist_ok=True)
    jobs = [(f, c) for c in base.CONFIGS for f in base.FIRES]
    with ThreadPoolExecutor(max_workers=3) as ex:
        rows = list(ex.map(base.run, jobs))
    (base.EXP / "exp28_containment_op.json").write_text(json.dumps(rows, indent=1))
