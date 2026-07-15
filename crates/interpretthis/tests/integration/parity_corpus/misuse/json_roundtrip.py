import json
data = {"name": "test", "values": [1, 2, 3], "nested": {"a": True, "b": None}}
s = json.dumps(data)
back = json.loads(s)
print(back == data)
print(json.dumps([1, 2, 3], separators=(",", ":")))
print(json.dumps({"b": 1, "a": 2}, sort_keys=True))
print(json.loads('{"x": 1.5, "y": [true, false, null]}'))
print(json.dumps("héllo", ensure_ascii=False))
print(json.dumps({"k": "v"}, indent=2))
