# statistics.median validates that every data point is numeric. Regression: a
# non-numeric element became NaN in the sort key and was silently tolerated,
# so median([1, 2, "x"]) returned 2.
import statistics

print(statistics.median([3, 1, 2]))        # 2 (odd -> middle, int preserved)
print(statistics.median([1, 2, 3, 4]))     # 2.5 (even -> float average)
print(statistics.median([True, 2, 3]))     # bool counts as int

try:
    statistics.median([1, 2, "x"])
except TypeError:
    print("TypeError")
try:
    statistics.median([None, 1])
except TypeError:
    print("TypeError")
