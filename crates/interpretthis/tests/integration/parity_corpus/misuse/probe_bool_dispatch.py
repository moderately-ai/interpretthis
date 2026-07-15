class Truthy:
    def __init__(self, v):
        self.v = v
    def __bool__(self):
        return self.v > 0
print(all([Truthy(1), Truthy(2), Truthy(3)]))
print(all([Truthy(1), Truthy(-1)]))
print(any([Truthy(-1), Truthy(-2), Truthy(1)]))
print(any([Truthy(-1), Truthy(-2)]))
class Sized:
    def __init__(self, n):
        self.n = n
    def __len__(self):
        return self.n
print(bool(Sized(0)), bool(Sized(5)))
print(all([Sized(1), Sized(2)]))
print(any([Sized(0), Sized(0)]))
items = [Truthy(1), Truthy(0), Truthy(2)]
print([bool(x) for x in items])
print(len([x for x in items if x]))
