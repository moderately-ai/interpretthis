class Range3:
    def __init__(self, n):
        self.n = n
    def __iter__(self):
        self.i = 0
        return self
    def __next__(self):
        if self.i >= self.n:
            raise StopIteration
        self.i += 1
        return self.i
r = Range3(3)
print(list(r))
print([x*2 for x in Range3(3)])
total = 0
for x in Range3(4):
    total += x
print(total)
print(sum(Range3(5)))
print(tuple(Range3(2)))
print(max(Range3(3)))
a, b, c = Range3(3)
print(a, b, c)
