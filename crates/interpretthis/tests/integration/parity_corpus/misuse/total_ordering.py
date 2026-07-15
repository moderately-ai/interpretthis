from functools import total_ordering
@total_ordering
class Ver:
    def __init__(self, v):
        self.v = v
    def __lt__(self, o):
        return self.v < o.v
    def __eq__(self, o):
        return self.v == o.v
print(Ver(1) <= Ver(2))
print(Ver(3) >= Ver(2))
print(Ver(2) <= Ver(2))
print(Ver(2) >= Ver(2))
print(Ver(3) > Ver(1))
print(Ver(1) < Ver(2))
print(Ver(5) > Ver(2))
print(sorted([Ver(3), Ver(1), Ver(2)], key=lambda x: x.v)[0].v)

@total_ordering
class Rev:
    def __init__(self, v):
        self.v = v
    def __eq__(self, o):
        return self.v == o.v
    def __gt__(self, o):
        return self.v > o.v
print(Rev(1) < Rev(2))
print(Rev(2) <= Rev(3))
print(Rev(3) >= Rev(3))
