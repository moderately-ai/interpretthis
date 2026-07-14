# operator.itemgetter / attrgetter / methodcaller return callables, chiefly used
# as sorted/min/max keys.
from operator import itemgetter, attrgetter, methodcaller

data = [{"name": "b", "age": 2}, {"name": "a", "age": 3}, {"name": "c", "age": 1}]
print(sorted(data, key=itemgetter("age")))
print(sorted(data, key=itemgetter("name"))[0])
print(itemgetter(1)([10, 20, 30]))
print(itemgetter(0, 2)([10, 20, 30]))


class P:
    def __init__(self, x, y):
        self.x, self.y = x, y

    def dist(self):
        return self.x + self.y

    def scale(self, k):
        return self.x * k


pts = [P(1, 5), P(3, 1)]
print(attrgetter("x")(pts[0]))
print([attrgetter("x")(p) for p in pts])
print(attrgetter("x", "y")(pts[0]))
print(sorted(pts, key=attrgetter("x"))[0].x)
print(methodcaller("dist")(pts[0]))
print(methodcaller("scale", 10)(pts[1]))


# Dotted attribute path.
class Q:
    def __init__(self):
        self.inner = P(7, 8)


print(attrgetter("inner.x")(Q()))
print(attrgetter("inner.x", "inner.y")(Q()))

# itemgetter() with no args raises.
try:
    itemgetter()
except TypeError:
    print("TypeError")
