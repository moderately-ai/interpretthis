import math
for f, a in [("sqrt", -1), ("log", -1), ("log", 0), ("acos", 2), ("asin", -2)]:
    try:
        getattr(math, f)(a)
    except ValueError:
        print(f"{f}({a}) ValueError")
try:
    print(1 / 0)
except ZeroDivisionError:
    print("zerodiv")
try:
    print(math.sqrt(-1))
except ValueError as e:
    print("sqrt neg")
print(math.inf, math.nan != math.nan)
print(math.isinf(math.inf), math.isnan(math.nan))
try:
    math.factorial(-1)
except ValueError:
    print("factorial neg")
print(math.pow(0, 0), math.pow(2, -1))
try:
    print(0 ** -1)
except ZeroDivisionError:
    print("0**-1")
print(2 ** -2)
print((-8) ** (1/3))
print(math.log(math.e), math.log(100, 10))
