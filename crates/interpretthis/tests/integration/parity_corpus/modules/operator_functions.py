# operator module: the functional forms of the operators, used directly and as
# reduce/accumulate/sorted keys. Regression: the module did not exist.
import operator as op
from functools import reduce

print(op.add(2, 3), op.sub(5, 2), op.mul(4, 3), op.truediv(7, 2), op.floordiv(7, 2), op.mod(7, 3), op.pow(2, 10))
print(op.and_(6, 3), op.or_(4, 1), op.xor(5, 3), op.lshift(1, 4), op.rshift(16, 2))
print(op.lt(1, 2), op.le(2, 2), op.eq(2, 2), op.ne(1, 2), op.gt(3, 2), op.ge(2, 3))
print(op.neg(5), op.pos(-3), op.invert(5), op.abs(-4), op.index(True))
print(op.not_(0), op.not_(5), op.truth([]), op.truth([1]))
print(op.is_(None, None), op.is_not(1, 2))
print(op.contains([1, 2, 3], 2), op.getitem([10, 20, 30], 1), op.concat([1], [2]))
print(op.countOf([1, 2, 2, 3], 2), op.indexOf([1, 2, 3], 3))
print(reduce(op.add, [1, 2, 3, 4]))
print(reduce(op.mul, [1, 2, 3, 4], 1))
print(op.add(1.5, 2), op.add("a", "b"), op.mul([0], 3))


# Works with a user class's dunders.
class V:
    def __init__(self, n):
        self.n = n

    def __add__(self, other):
        return V(self.n + other.n)

    def __lt__(self, other):
        return self.n < other.n

    def __repr__(self):
        return f"V({self.n})"


print(op.add(V(2), V(3)))
print(op.lt(V(1), V(2)), op.lt(V(3), V(2)))
