# Pins: json.dumps(obj, indent=N) emits a multi-line indented form
# matching CPython exactly (key:value separator with space, items
# separator with newline + indent).
import json
print(json.dumps({"a": 1, "b": [1, 2, 3]}, indent=2))
