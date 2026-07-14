# math.exp/log/pow raise on domain and range errors instead of returning
# inf/nan. Regression: exp(1000) -> inf, pow(0, -1) -> inf, pow(-1, 0.5) -> nan,
# log(8, 0) all passed through silently.
import math

print(math.pow(2, 10))          # 1024.0 (ok)
print(math.exp(0))              # 1.0 (ok)
print(math.log(8, 2))           # 3.0 (ok)

for label, fn in [
    ("pow0neg", lambda: math.pow(0, -1)),
    ("pow_neg_frac", lambda: math.pow(-1, 0.5)),
    ("log_base0", lambda: math.log(8, 0)),
]:
    try:
        fn()
    except ValueError:
        print(label, "ValueError")

for label, fn in [
    ("exp_big", lambda: math.exp(1000)),
    ("pow_over", lambda: math.pow(2, 10000)),
]:
    try:
        fn()
    except OverflowError:
        print(label, "OverflowError")

try:
    math.log(8, 1)
except ZeroDivisionError:
    print("log_base1 ZeroDivisionError")

# Infinite operands are not range errors.
print(math.exp(float("inf")))   # inf
print(math.pow(float("inf"), 0))  # 1.0
