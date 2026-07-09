# Round-trip preserves bool/None/float identity at the Python level.
import json
original = {"a": True, "b": False, "c": None, "d": [1, 2.5]}
print(json.loads(json.dumps(original)))
