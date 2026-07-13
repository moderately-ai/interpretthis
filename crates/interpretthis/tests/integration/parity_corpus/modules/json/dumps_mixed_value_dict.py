# Mixed scalar values exercise each leaf encoder in the same payload.
import json
print(json.dumps({"yes": True, "no": False, "x": None, "n": 42}))
