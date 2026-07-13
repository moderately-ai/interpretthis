# Pins: user-class iteration protocol. The container's __iter__
# returns an iterator object whose __next__ yields items and raises
# StopIteration to terminate. Customer pattern: custom collection
# wrappers (paginated queries, lazy sequences).
class CountUp:
    def __init__(self, n):
        self.n = n
    def __iter__(self):
        return CountUpIter(self.n)

class CountUpIter:
    def __init__(self, n):
        self.n = n
        self.i = 0
    def __next__(self):
        if self.i >= self.n:
            raise StopIteration
        v = self.i
        self.i += 1
        return v

# for-loop drives __iter__/__next__
for x in CountUp(3):
    print(x)

# list() materializes through the same protocol
print(list(CountUp(4)))
