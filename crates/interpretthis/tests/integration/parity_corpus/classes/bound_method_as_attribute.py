# Accessing an instance method as a bare attribute (no call) yields a bound
# method that carries the instance. Storing it and calling later dispatches
# against the same instance, and mutations to self propagate. Regression: the
# attribute path only searched class-level *data* attributes, never the method
# table, so `p.go` raised AttributeError instead of binding the method.
class Point:
    def __init__(self, x):
        self.x = x

    def go(self, dx):
        self.x += dx
        return self.x

    def value(self):
        return self.x


p = Point(10)
m = p.go
print(m(5))          # 15 — self bound to p
print(m(2))          # 17
print(p.x)           # 17 — mutation propagated back to p
print(p.value())     # 17

# A bound method is a first-class value: pass it around.
def apply_twice(f, a):
    f(a)
    return f(a)


print(apply_twice(p.go, 1))   # 19
print(p.x)                     # 19

# Bound method through a container element.
pts = [Point(0), Point(100)]
getter = pts[1].value
print(getter())                # 100
