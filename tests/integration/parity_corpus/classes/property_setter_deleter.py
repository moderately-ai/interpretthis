# Pins: @property + @x.setter + @x.deleter all wired. Customer
# pattern for validated mutators with cleanup.
class Temp:
    def __init__(self):
        self._c = 0
    @property
    def c(self):
        return self._c
    @c.setter
    def c(self, v):
        self._c = max(0, v)
    @c.deleter
    def c(self):
        self._c = 0

t = Temp()
t.c = 25
print(t.c)
t.c = -5
print(t.c)
del t.c
print(t.c)
t.c = 100
print(t.c)
