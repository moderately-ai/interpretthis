# sort_keys=True reorders flat dict keys alphabetically before emission.
import json
print(json.dumps({"b": 2, "a": 1, "c": 3}, sort_keys=True))
