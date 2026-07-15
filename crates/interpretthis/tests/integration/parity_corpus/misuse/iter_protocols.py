class Countdown:
    def __init__(self, n):
        self.n = n
    def __iter__(self):
        return self
    def __next__(self):
        if self.n <= 0:
            raise StopIteration
        self.n -= 1
        return self.n + 1
print(list(Countdown(3)))
print([x for x in Countdown(4)])
print(sum(Countdown(5)))
class Range2:
    def __init__(self, stop):
        self.stop = stop
    def __getitem__(self, i):
        if i >= self.stop:
            raise IndexError
        return i * 10
print(list(Range2(3)))
a, b, c = Countdown(3)
print(a, b, c)
