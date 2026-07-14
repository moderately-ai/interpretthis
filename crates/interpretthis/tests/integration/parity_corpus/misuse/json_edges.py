import json
print(json.loads("12345678901234567890123456789"))
print(json.dumps({"a": 1, "b": [2, 3]}, indent=2))
print(json.dumps(["x"], ensure_ascii=False))
print(json.dumps({"z": 1, "a": 2}, sort_keys=True))
try:
    print(json.dumps({1, 2}))
except TypeError as e:
    print("set:", type(e).__name__)
