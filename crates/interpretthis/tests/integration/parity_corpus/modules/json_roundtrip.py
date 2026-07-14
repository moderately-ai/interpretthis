import json
print(json.dumps({"a": 1, "b": [2, 3], "c": None, "d": True}))
print(json.dumps([1, "two", 3.5, False]))
print(json.loads('{"x": 1, "y": [2, 3]}'))
print(json.loads('[1, 2.5, "three", null, true, false]'))
print(json.dumps({"key": "val"}, sort_keys=True))
print(json.dumps({"a": 1}, separators=(",", ":")))
print(json.loads('{"nested": {"deep": [1, 2]}}')["nested"]["deep"])
print(json.dumps("special: \" \\ \n"))
