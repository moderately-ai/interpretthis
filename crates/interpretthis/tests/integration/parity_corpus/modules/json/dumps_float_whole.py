# json.dumps on a whole-number float keeps the trailing `.0`.
import json
print(json.dumps(1.0))
