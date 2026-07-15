from collections import ChainMap
a = {"x": 1, "y": 2}
b = {"y": 20, "z": 30}
cm = ChainMap(a, b)
print(cm["x"], cm["y"], cm["z"])
print(sorted(cm.keys()))
print(sorted(cm.values()))
print(len(cm))
print("z" in cm, "w" in cm)
cm["x"] = 100
print(a["x"])
cm["new"] = 5
print(a["new"])
child = cm.new_child({"y": 999})
print(child["y"])
print(cm.get("missing", "default"))
print(dict(cm) == {"x": 100, "y": 2, "z": 30, "new": 5})
