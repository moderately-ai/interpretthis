# __iter__ / __next__ custom iterators, StopIteration, iterator exhaustion.
class Count:
    def __init__(self, n):
        self.n = n
        self.i = 0
    def __iter__(self):
        return self
    def __next__(self):
        if self.i >= self.n:
            raise StopIteration
        self.i += 1
        return self.i

print(list(Count(3)))
print([x * 2 for x in Count(4)])
print(sum(Count(5)), max(Count(5)), min(Count(5)))

c = Count(2)
it = iter(c)
print(next(it), next(it))
try:
    next(it)
except StopIteration:
    print("exhausted")

# __iter__ returning a separate iterator
class Range3:
    def __iter__(self):
        return iter([10, 20, 30])

print(list(Range3()), 20 in Range3())

# __getitem__-based iteration (no __iter__)
class Seq:
    def __init__(self, data):
        self.data = data
    def __getitem__(self, i):
        return self.data[i]

print(list(Seq([1, 2, 3])), [x for x in Seq(["a", "b"])])

# generator-based __iter__
class Fib:
    def __init__(self, n):
        self.n = n
    def __iter__(self):
        a, b = 0, 1
        for _ in range(self.n):
            yield a
            a, b = b, a + b

print(list(Fib(8)))

# zip / enumerate over custom iterators
print(list(zip(Count(3), "abc")))
print(list(enumerate(Count(3), start=10)))
