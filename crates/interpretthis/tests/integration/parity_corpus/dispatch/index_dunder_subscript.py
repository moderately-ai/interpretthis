# Sequence subscripting and slicing accept any object with __index__
# (CPython's operator.index).
class Idx:
    def __init__(self, n):
        self.n = n
    def __index__(self):
        return self.n

print([10, 20, 30, 40][Idx(2)])
print((1, 2, 3)[Idx(0)])
print("hello"[Idx(1)])
print(b"abc"[Idx(2)])
print(list(range(10))[Idx(5)])
print([1, 2, 3, 4, 5][Idx(-1)])
print([1, 2, 3, 4, 5][Idx(1):Idx(4)])
print("abcdef"[Idx(1):Idx(5):Idx(2)])

class NoIdx:
    pass

try:
    [1, 2, 3][NoIdx()]
except TypeError as e:
    print(type(e).__name__)

class BadIdx:
    def __index__(self):
        return "nope"

try:
    [1, 2, 3][BadIdx()]
except TypeError as e:
    print(type(e).__name__)
