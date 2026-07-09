# Pins: `dumps = json.dumps; dumps({"x": 1})` — ModuleFunction stored
# under a name then called via the eval_call variable-lookup branch
# (which already handles ModuleFunction, but pinning here so a regression
# would surface immediately).
import json
dumps = json.dumps
print(dumps({"x": 1}))
