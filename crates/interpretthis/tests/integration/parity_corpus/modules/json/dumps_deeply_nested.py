# Dict/list interleaving validates the encoder transitions between container kinds.
import json
print(json.dumps({"a": {"b": {"c": [1, 2, {"d": True}]}}}))
