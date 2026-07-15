class Fib:
    def __init__(self, n):
        self.n = n
    def __iter__(self):
        a, b = 0, 1
        for _ in range(self.n):
            yield a
            a, b = b, a + b
f = Fib(8)
print(list(f))
print(sum(Fib(5)))
print(max(Fib(6)))
print([x for x in Fib(5)])
print(sorted(Fib(7)))
class CountUp:
    def __init__(self, limit):
        self.limit = limit
        self.i = 0
    def __iter__(self):
        return self
    def __next__(self):
        if self.i >= self.limit:
            raise StopIteration
        self.i += 1
        return self.i
print(list(CountUp(4)))
c = CountUp(3)
print(next(c), next(c))
print(tuple(Fib(4)))
print(", ".join(str(x) for x in Fib(5)))
print(3 in Fib(6))
