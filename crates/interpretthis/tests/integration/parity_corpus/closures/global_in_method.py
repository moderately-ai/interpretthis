# `global` inside a class method behaves the same as in a function:
# the assignment targets module scope. The checkpoint must respect
# `global` declarations even when the body is a method call frame.
hits = 0

class Counter:
    def hit(self):
        global hits
        hits = hits + 1

c = Counter()
c.hit()
c.hit()
c.hit()
print(hits)
