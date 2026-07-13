# json.dumps preserves the leading minus on negative integers.
import json
print(json.dumps(-42))
