# Integer dict keys are coerced to their decimal string form before encoding.
import json
print(json.dumps({1: "one", 2: "two"}))
