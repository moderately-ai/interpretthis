class R:
    def __init__(self, n): self.n = n
    def __iter__(self):
        i = 0
        while i < self.n:
            yield i
            i += 1
print([*R(3)])
print([*R(2), *R(2)])
print((*R(3),))
print({*R(3)})
print([0, *R(3), 99])
class Fixed:
    def __iter__(self): return iter(["a", "b", "c"])
print([*Fixed()])
print({*Fixed(), *R(2)})
