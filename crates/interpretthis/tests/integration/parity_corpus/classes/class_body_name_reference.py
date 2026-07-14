# A class body is a code block: a statement can read names bound earlier in the
# same body. Regression: top-level class-body assignments were written straight
# to the class dict and never into the executing namespace, so a later
# expression referencing an earlier class attribute raised NameError.
class C:
    x = 1
    y = x + 1
    z = x + y
    label = "v" + str(z)
    doubled = [x, y, z]
    total = sum(doubled)


print(C.x, C.y, C.z)
print(C.label)
print(C.doubled)
print(C.total)


# An enclosing name is readable from the class body, and later statements build
# on earlier class attributes. (A comprehension body has its own scope and can
# NOT see class-level names — that CPython gotcha is intentionally not exercised
# here; the iterable of the outermost `for` is evaluated in the class scope.)
BASE = 100


class D:
    offset = 5
    base_plus = BASE + offset
    values = list(range(offset))
    first = values[0]


print(D.base_plus, D.values, D.first)


# A method reads its class attributes through the instance, not the class-body
# scope (the class body's names do not leak into method bodies).
class E:
    factor = 3

    def scale(self, n):
        return n * self.factor


print(E().scale(4))
