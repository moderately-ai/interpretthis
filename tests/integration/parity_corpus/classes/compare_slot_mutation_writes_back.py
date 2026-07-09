# An `__lt__` (or other rich-compare slot) that mutates `self` must
# propagate the mutation to the originating variable — CPython binds
# `self` to the live instance via cell semantics. Our flat owned-Value
# model used to drop the mutated `self` returned by call_method after
# the comparison.
class CompareCount:
    def __init__(self):
        self.calls = 0
    def __lt__(self, other):
        self.calls = self.calls + 1
        return False
    def __gt__(self, other):
        return False

c = CompareCount()
_ = c < 1 < 2
print(c.calls)
