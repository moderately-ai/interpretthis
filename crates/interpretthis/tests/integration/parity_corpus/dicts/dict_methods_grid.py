# Pins: dict.get / .pop / .update / .setdefault. Heavy customer
# pattern.
d = {"a": 1, "b": 2}
print(d.get("a"))
print(d.get("c", "default"))
print(d.pop("a"))
print(d)
d.update({"c": 3, "d": 4})
print(d)
print(d.setdefault("e", 5))
print(d)
