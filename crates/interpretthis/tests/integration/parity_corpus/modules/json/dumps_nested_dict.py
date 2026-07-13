# Recursive dump of nested dicts must thread the default separators all the way down.
import json
print(json.dumps({"a": {"b": {"c": 3}}}))
