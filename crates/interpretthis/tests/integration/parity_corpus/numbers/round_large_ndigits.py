# round(x, n) with a very negative n rounds to zero rather than producing NaN.
# Regression: the float path computed 10**n as a factor, which underflowed to
# 0.0 for large |n|, so (x*0)/0 == NaN.
print(round(123.456, -400))     # 0.0
print(round(-123.456, -400))    # -0.0 (sign preserved)
print(round(0.0, -400))         # 0.0
print(round(125.0, -1))         # 120.0
print(round(123.456, -1))       # 120.0
print(round(123.456, 400))      # 123.456 (large positive n is a no-op)
print(round(2.675, 2))          # 2.67 (correctly-rounded decimal)
print(round(1234, -2))          # 1200 (int path unchanged)
