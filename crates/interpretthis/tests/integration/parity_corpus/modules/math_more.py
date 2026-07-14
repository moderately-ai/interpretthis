import math
print(math.comb(5, 2), math.comb(10, 0), math.comb(3, 5))
print(math.comb(100, 50))                       # arbitrary precision
print(math.perm(5, 2), math.perm(4), math.perm(3, 5))
print(math.prod([1, 2, 3, 4]), math.prod([], start=10), math.prod([2, 3], start=5))
print(math.dist([0, 0], [3, 4]), math.dist([1], [4]))
print(math.lcm(4, 6), math.lcm(), math.lcm(0, 5), math.lcm(3, 4, 5))
print(math.isclose(1.0, 1.0), math.isclose(1.0, 1.0000001))
print(math.isclose(1.0, 1.1, rel_tol=0.2))
print(round(math.expm1(0.0), 6), round(math.log1p(0.0), 6))
