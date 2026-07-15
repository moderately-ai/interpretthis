class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y
p = Point(3, 4)
print(vars(p))
print(vars(p) == {"x": 3, "y": 4})
p.z = 10
print(sorted(vars(p).items()))
class Empty: pass
print(vars(Empty()))
# non-instance forms raise TypeError
for bad in (1, "s", [1], {}):
    try:
        vars(bad)
    except TypeError:
        print("rejected", type(bad).__name__)
