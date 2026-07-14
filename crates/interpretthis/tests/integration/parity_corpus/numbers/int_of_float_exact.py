# int(float) and round(float) convert to the EXACT integer, raise on nan/inf.
# Regression: the converter saturated at i64 bounds and mapped NaN to 0, so
# int(1e30) gave i64::MAX and int(float('nan')) gave 0.
print(int(1e30))
print(int(-1e30))
print(int(2.9))
print(int(-2.9))
print(round(2.5))
print(round(1e30))

for label, thunk in [
    ("int-nan", lambda: int(float("nan"))),
    ("int-inf", lambda: int(float("inf"))),
    ("round-nan", lambda: round(float("nan"))),
]:
    try:
        thunk()
        print(label, "NO ERROR")
    except ValueError:
        print(label, "ValueError")
    except OverflowError:
        print(label, "OverflowError")
