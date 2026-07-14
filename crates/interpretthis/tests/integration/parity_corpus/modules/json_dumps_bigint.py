# json.dumps serializes integers beyond i64 exactly. Regression: a BigInt hit
# the catch-all and raised "Object of type int is not JSON serializable".
import json

print(json.dumps(10**30))
print(json.dumps(-(2**80)))
print(json.dumps([1, 10**25, 2]))
print(json.dumps({"n": 99999999999999999999999}))
print(json.dumps({"a": 10**30}, indent=2))

# Round-trips exactly.
print(json.loads(json.dumps(12345678901234567890123456789)))
