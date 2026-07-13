# Pin: a class with `__init__` storing an attribute on `self`, then reading it
# back through an instance.
# Expected stdout: `5`.
class P:
    def __init__(self, x):
        self.x = x


p = P(5)
print(p.x)
