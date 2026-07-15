d1 = {"a": 1}
d2 = d1
d1["b"] = 2
print(sorted(d2.items()))
def mutate(d):
    d["c"] = 3
    d.update({"e": 5})
mutate(d1)
print(sorted(d1.items()))
print(d1 is d2)
nested = {"inner": {}}
ref = nested["inner"]
ref["x"] = 10
print(nested)
d3 = dict(d1)
d3["z"] = 99
print("z" in d1)
import copy
d4 = copy.copy(d1)
d4["w"] = 1
print("w" in d1)
class Box:
    def __init__(self):
        self.data = {}
b = Box()
alias = b.data
alias["k"] = "v"
print(b.data)
