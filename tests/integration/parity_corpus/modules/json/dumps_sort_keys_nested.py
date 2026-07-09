# sort_keys must recurse into nested dicts, not just the top level.
import json
print(json.dumps({"z": {"b": 2, "a": 1}}, sort_keys=True))
