class Fib:
    def __init__(self, n):
        self.n = n
        self.a, self.b = 0, 1
        self.count = 0
    def __iter__(self):
        return self
    def __next__(self):
        if self.count >= self.n:
            raise StopIteration
        self.count += 1
        result = self.a
        self.a, self.b = self.b, self.a + self.b
        return result
print(list(Fib(8)))
class Countdown:
    def __init__(self, start):
        self.start = start
    def __iter__(self):
        n = self.start
        while n > 0:
            yield n
            n -= 1
print(list(Countdown(5)))
for x in Countdown(3):
    print(x)
class Repeater:
    def __init__(self, val, times):
        self.val = val
        self.times = times
    def __iter__(self):
        for _ in range(self.times):
            yield self.val
print(list(Repeater("x", 3)))
print(sum(Fib(10)))
